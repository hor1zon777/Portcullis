//! SQLite 持久化层。
//!
//! 提供迁移、种子数据、站点 CRUD、IP 名单、请求日志、防重放 的数据库操作。
//! 所有函数都是同步的（rusqlite），调用方通过 `spawn_blocking` 在 tokio 中使用。

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{params, Connection};

use crate::admin::request_log::LogEntry;
use crate::config::SiteConfig;

pub type Db = Arc<Mutex<Connection>>;

/// 安全获取 DB 锁：mutex 中毒时恢复而非 panic。
fn lock(db: &Db) -> std::sync::MutexGuard<'_, Connection> {
    db.lock().unwrap_or_else(|e| e.into_inner())
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

pub fn open(path: &Path) -> Db {
    let conn = Connection::open(path).expect("无法打开 SQLite 数据库");
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=5000;")
        .expect("PRAGMA 设置失败");
    Arc::new(Mutex::new(conn))
}

pub fn open_memory() -> Db {
    let conn = Connection::open_in_memory().expect("无法打开内存数据库");
    Arc::new(Mutex::new(conn))
}

// ──────── 迁移 ────────

pub fn migrate(db: &Db) {
    let conn = lock(db);
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sites (
            key        TEXT PRIMARY KEY,
            secret_key TEXT    NOT NULL,
            diff       INTEGER NOT NULL DEFAULT 18,
            origins    TEXT    NOT NULL DEFAULT '[]',
            argon2_m_cost INTEGER NOT NULL DEFAULT 19456,
            argon2_t_cost INTEGER NOT NULL DEFAULT 2,
            argon2_p_cost INTEGER NOT NULL DEFAULT 1,
            bind_token_to_ip INTEGER NOT NULL DEFAULT 0,
            bind_token_to_ua INTEGER NOT NULL DEFAULT 0,
            secret_key_hashed INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS ip_lists (
            ip_or_cidr TEXT NOT NULL,
            list_type  TEXT NOT NULL CHECK(list_type IN ('blocked','allowed')),
            created_at INTEGER NOT NULL,
            PRIMARY KEY (ip_or_cidr, list_type)
        );

        CREATE TABLE IF NOT EXISTS request_log (
            id          INTEGER PRIMARY KEY AUTOINCREMENT,
            timestamp   INTEGER NOT NULL,
            ip          TEXT,
            site_key    TEXT    NOT NULL,
            nonce       INTEGER NOT NULL,
            success     INTEGER NOT NULL,
            duration_ms REAL    NOT NULL,
            error       TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_log_ts ON request_log(timestamp);

        CREATE TABLE IF NOT EXISTS replay_nonces (
            id         TEXT NOT NULL,
            kind       TEXT NOT NULL CHECK(kind IN ('challenge','token')),
            expires_ms INTEGER NOT NULL,
            PRIMARY KEY (id, kind)
        );
        CREATE INDEX IF NOT EXISTS idx_replay_exp ON replay_nonces(expires_ms);

        CREATE TABLE IF NOT EXISTS server_secrets (
            key        TEXT PRIMARY KEY,
            value      BLOB NOT NULL,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS admin_audit (
            id           INTEGER PRIMARY KEY AUTOINCREMENT,
            ts           INTEGER NOT NULL,
            token_prefix TEXT,
            action       TEXT    NOT NULL,
            target       TEXT,
            ip           TEXT,
            success      INTEGER NOT NULL,
            meta_json    TEXT
        );
        CREATE INDEX IF NOT EXISTS idx_audit_ts ON admin_audit(ts);
        CREATE INDEX IF NOT EXISTS idx_audit_action ON admin_audit(action);
        ",
    )
    .expect("数据库迁移失败");

    // v1.3.0 增量迁移：为已有 sites 表添加 Argon2 参数列
    for (col, default) in [
        ("argon2_m_cost", "19456"),
        ("argon2_t_cost", "2"),
        ("argon2_p_cost", "1"),
    ] {
        let sql = format!("ALTER TABLE sites ADD COLUMN {col} INTEGER NOT NULL DEFAULT {default}");
        // 列已存在时 ALTER 会报错，静默忽略
        let _ = conn.execute(&sql, []);
    }

    // v1.4.0 增量迁移：为已有 sites 表添加身份绑定开关列
    for col in ["bind_token_to_ip", "bind_token_to_ua"] {
        let sql = format!("ALTER TABLE sites ADD COLUMN {col} INTEGER NOT NULL DEFAULT 0");
        let _ = conn.execute(&sql, []);
    }

    // v1.5.0 增量迁移：为已有 sites 表添加 secret_key_hashed 标志列。
    // 真正的 secret_key 明文 → HMAC 转换在 `migrate_site_secret_keys` 里完成，
    // 该函数需要 master_secret，因此在 `AppState::new` 里单独调用。
    let _ = conn.execute(
        "ALTER TABLE sites ADD COLUMN secret_key_hashed INTEGER NOT NULL DEFAULT 0",
        [],
    );
}

/// v1.5.0：把 `sites` 表中 `secret_key_hashed = 0` 的行的 `secret_key` 做
/// `HMAC-SHA256(master, plain)` 并 base64 化，然后置 `secret_key_hashed = 1`。
/// 幂等：已 hashed 的行跳过。
pub fn migrate_site_secret_keys(db: &Db, master: &[u8]) {
    let conn = lock(db);

    // 收集需要迁移的行
    let rows: Vec<(String, String)> = {
        let mut stmt =
            match conn.prepare("SELECT key, secret_key FROM sites WHERE secret_key_hashed = 0") {
                Ok(s) => s,
                Err(e) => {
                    tracing::warn!("secret_key 迁移查询失败: {e}");
                    return;
                }
            };
        let iter = match stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) {
            Ok(i) => i,
            Err(e) => {
                tracing::warn!("secret_key 迁移查询失败: {e}");
                return;
            }
        };
        iter.filter_map(Result::ok).collect()
    };

    let count = rows.len();
    for (key, plain) in rows {
        let hashed = crate::site_secret::hash(&plain, master);
        let _ = conn.execute(
            "UPDATE sites SET secret_key = ?1, secret_key_hashed = 1, updated_at = ?2 WHERE key = ?3",
            params![hashed, now_ms(), key],
        );
    }

    if count > 0 {
        tracing::info!(
            migrated = count,
            "v1.5.0: 已将 {count} 个站点的 secret_key 迁移到 HMAC 存储"
        );
    }
}

// ──────── 种子数据 ────────

pub fn seed_sites(db: &Db, sites: &HashMap<String, SiteConfig>) {
    let conn = lock(db);
    let now = now_ms();
    let mut stmt = conn
        .prepare("INSERT OR IGNORE INTO sites (key, secret_key, diff, origins, argon2_m_cost, argon2_t_cost, argon2_p_cost, bind_token_to_ip, bind_token_to_ua, secret_key_hashed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)")
        .unwrap();
    for (key, site) in sites {
        let origins_json = serde_json::to_string(&site.origins).unwrap_or_else(|_| "[]".into());
        stmt.execute(params![
            key,
            site.secret_key,
            site.diff,
            origins_json,
            site.argon2_m_cost,
            site.argon2_t_cost,
            site.argon2_p_cost,
            site.bind_token_to_ip as i32,
            site.bind_token_to_ua as i32,
            site.secret_key_hashed as i32,
            now,
            now
        ])
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
    }
}

pub fn seed_ip_lists(db: &Db, blocked: &[String], allowed: &[String]) {
    let conn = lock(db);
    let now = now_ms();
    let mut stmt = conn
        .prepare("INSERT OR IGNORE INTO ip_lists (ip_or_cidr, list_type, created_at) VALUES (?1, ?2, ?3)")
        .unwrap();
    for ip in blocked {
        stmt.execute(params![ip, "blocked", now])
            .unwrap_or_else(|e| {
                tracing::warn!("DB 写入失败: {e}");
                0
            });
    }
    for ip in allowed {
        stmt.execute(params![ip, "allowed", now])
            .unwrap_or_else(|e| {
                tracing::warn!("DB 写入失败: {e}");
                0
            });
    }
}

// ──────── 加载 ────────

pub fn load_sites(db: &Db) -> HashMap<String, SiteConfig> {
    let conn = lock(db);
    let mut stmt = conn
        .prepare("SELECT key, secret_key, diff, origins, argon2_m_cost, argon2_t_cost, argon2_p_cost, bind_token_to_ip, bind_token_to_ua, secret_key_hashed FROM sites")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let secret_key: String = row.get(1)?;
            let diff: u8 = row.get(2)?;
            let origins_json: String = row.get(3)?;
            let origins: Vec<String> = serde_json::from_str(&origins_json).unwrap_or_default();
            let argon2_m_cost: u32 = row.get(4)?;
            let argon2_t_cost: u32 = row.get(5)?;
            let argon2_p_cost: u32 = row.get(6)?;
            let bind_token_to_ip: i32 = row.get(7)?;
            let bind_token_to_ua: i32 = row.get(8)?;
            let secret_key_hashed: i32 = row.get(9)?;
            Ok((
                key,
                SiteConfig {
                    secret_key,
                    diff,
                    origins,
                    argon2_m_cost,
                    argon2_t_cost,
                    argon2_p_cost,
                    bind_token_to_ip: bind_token_to_ip != 0,
                    bind_token_to_ua: bind_token_to_ua != 0,
                    secret_key_hashed: secret_key_hashed != 0,
                },
            ))
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

pub fn load_ip_list(db: &Db, list_type: &str) -> Vec<String> {
    let conn = lock(db);
    let mut stmt = conn
        .prepare("SELECT ip_or_cidr FROM ip_lists WHERE list_type = ?1")
        .unwrap();
    let rows = stmt
        .query_map(params![list_type], |row| row.get::<_, String>(0))
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

// ──────── 站点 CRUD ────────

pub fn insert_site(db: &Db, key: &str, site: &SiteConfig) {
    let conn = lock(db);
    let now = now_ms();
    let origins_json = serde_json::to_string(&site.origins).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT OR REPLACE INTO sites (key, secret_key, diff, origins, argon2_m_cost, argon2_t_cost, argon2_p_cost, bind_token_to_ip, bind_token_to_ua, secret_key_hashed, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
        params![
            key,
            site.secret_key,
            site.diff,
            origins_json,
            site.argon2_m_cost,
            site.argon2_t_cost,
            site.argon2_p_cost,
            site.bind_token_to_ip as i32,
            site.bind_token_to_ua as i32,
            site.secret_key_hashed as i32,
            now,
            now
        ],
    )
    .unwrap_or_else(|e| { tracing::warn!("DB 写入失败: {e}"); 0 });
}

pub fn update_site_fields(
    db: &Db,
    key: &str,
    diff: Option<u8>,
    origins: Option<&[String]>,
    argon2_m_cost: Option<u32>,
    argon2_t_cost: Option<u32>,
    argon2_p_cost: Option<u32>,
    bind_token_to_ip: Option<bool>,
    bind_token_to_ua: Option<bool>,
) {
    let conn = lock(db);
    let now = now_ms();
    if let Some(d) = diff {
        conn.execute(
            "UPDATE sites SET diff = ?1, updated_at = ?2 WHERE key = ?3",
            params![d, now, key],
        )
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
    }
    if let Some(o) = origins {
        let json = serde_json::to_string(o).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE sites SET origins = ?1, updated_at = ?2 WHERE key = ?3",
            params![json, now, key],
        )
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
    }
    if let Some(m) = argon2_m_cost {
        conn.execute(
            "UPDATE sites SET argon2_m_cost = ?1, updated_at = ?2 WHERE key = ?3",
            params![m, now, key],
        )
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
    }
    if let Some(t) = argon2_t_cost {
        conn.execute(
            "UPDATE sites SET argon2_t_cost = ?1, updated_at = ?2 WHERE key = ?3",
            params![t, now, key],
        )
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
    }
    if let Some(p) = argon2_p_cost {
        conn.execute(
            "UPDATE sites SET argon2_p_cost = ?1, updated_at = ?2 WHERE key = ?3",
            params![p, now, key],
        )
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
    }
    if let Some(b) = bind_token_to_ip {
        conn.execute(
            "UPDATE sites SET bind_token_to_ip = ?1, updated_at = ?2 WHERE key = ?3",
            params![b as i32, now, key],
        )
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
    }
    if let Some(b) = bind_token_to_ua {
        conn.execute(
            "UPDATE sites SET bind_token_to_ua = ?1, updated_at = ?2 WHERE key = ?3",
            params![b as i32, now, key],
        )
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
    }
}

pub fn delete_site(db: &Db, key: &str) {
    let conn = lock(db);
    conn.execute("DELETE FROM sites WHERE key = ?1", params![key])
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            0
        });
}

// ──────── IP 名单 ────────

pub fn insert_ip_list(db: &Db, ip: &str, list_type: &str) {
    let conn = lock(db);
    conn.execute(
        "INSERT OR IGNORE INTO ip_lists (ip_or_cidr, list_type, created_at) VALUES (?1, ?2, ?3)",
        params![ip, list_type, now_ms()],
    )
    .unwrap_or_else(|e| {
        tracing::warn!("DB 写入失败: {e}");
        0
    });
}

pub fn delete_ip_list(db: &Db, ip: &str, list_type: &str) {
    let conn = lock(db);
    conn.execute(
        "DELETE FROM ip_lists WHERE ip_or_cidr = ?1 AND list_type = ?2",
        params![ip, list_type],
    )
    .unwrap_or_else(|e| {
        tracing::warn!("DB 写入失败: {e}");
        0
    });
}

// ──────── 请求日志 ────────

pub fn insert_log(db: &Db, entry: &LogEntry) {
    let conn = lock(db);
    conn.execute(
        "INSERT INTO request_log (timestamp, ip, site_key, nonce, success, duration_ms, error) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![
            entry.timestamp as i64,
            entry.ip.map(|ip| ip.to_string()),
            entry.site_key,
            entry.nonce as i64,
            entry.success as i32,
            entry.duration_ms,
            entry.error,
        ],
    )
    .unwrap_or_else(|e| { tracing::warn!("DB 写入失败: {e}"); 0 });
}

pub fn load_recent_logs(db: &Db, limit: usize) -> Vec<LogEntry> {
    let conn = lock(db);
    let mut stmt = conn
        .prepare("SELECT timestamp, ip, site_key, nonce, success, duration_ms, error FROM request_log ORDER BY id DESC LIMIT ?1")
        .unwrap();
    let rows = stmt
        .query_map(params![limit as i64], |row| {
            let ip_str: Option<String> = row.get(1)?;
            Ok(LogEntry {
                timestamp: row.get::<_, i64>(0)? as u64,
                ip: ip_str.and_then(|s| s.parse().ok()),
                site_key: row.get(2)?,
                nonce: row.get::<_, i64>(3)? as u64,
                success: row.get::<_, i32>(4)? != 0,
                duration_ms: row.get(5)?,
                error: row.get(6)?,
            })
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

pub fn cleanup_old_logs(db: &Db, retention_days: u64) {
    let cutoff = now_ms() - (retention_days as i64 * 86_400_000);
    let conn = lock(db);
    conn.execute(
        "DELETE FROM request_log WHERE timestamp < ?1",
        params![cutoff],
    )
    .unwrap_or_else(|e| {
        tracing::warn!("DB 写入失败: {e}");
        0
    });
}

// ──────── 防重放 ────────

pub fn mark_nonce_used(db: &Db, id: &str, kind: &str, expires_ms: u64) -> bool {
    let conn = lock(db);
    let result = conn.execute(
        "INSERT OR IGNORE INTO replay_nonces (id, kind, expires_ms) VALUES (?1, ?2, ?3)",
        params![id, kind, expires_ms as i64],
    );
    matches!(result, Ok(1))
}

pub fn cleanup_expired_nonces(db: &Db) {
    let now = now_ms();
    let conn = lock(db);
    conn.execute(
        "DELETE FROM replay_nonces WHERE expires_ms <= ?1",
        params![now],
    )
    .unwrap_or_else(|e| {
        tracing::warn!("DB 写入失败: {e}");
        0
    });
}

// ──────── 服务端密钥（长寿 secret，比如 manifest signing key seed） ────────

/// 读取 32 字节秘密。不存在或长度不符返回 None。
pub fn load_server_secret_32(db: &Db, key: &str) -> Option<[u8; 32]> {
    let conn = lock(db);
    let value: Option<Vec<u8>> = conn
        .query_row(
            "SELECT value FROM server_secrets WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .ok();
    let bytes = value?;
    if bytes.len() != 32 {
        tracing::warn!(key, len = bytes.len(), "server_secrets 长度非 32，忽略");
        return None;
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(&bytes);
    Some(out)
}

/// 写入 / 覆盖 32 字节秘密（INSERT OR REPLACE）。
pub fn save_server_secret_32(db: &Db, key: &str, value: &[u8; 32]) {
    let conn = lock(db);
    conn.execute(
        "INSERT OR REPLACE INTO server_secrets (key, value, created_at) VALUES (?1, ?2, ?3)",
        params![key, value.as_slice(), now_ms()],
    )
    .unwrap_or_else(|e| {
        tracing::warn!("DB 写入失败: {e}");
        0
    });
}

/// 删除秘密，返回是否真删除了一行。
pub fn delete_server_secret(db: &Db, key: &str) -> bool {
    let conn = lock(db);
    conn.execute("DELETE FROM server_secrets WHERE key = ?1", params![key])
        .map(|n| n > 0)
        .unwrap_or_else(|e| {
            tracing::warn!("DB 写入失败: {e}");
            false
        })
}

// ──────── 管理员审计（v1.5.0） ────────

pub fn insert_audit(
    db: &Db,
    token_prefix: Option<&str>,
    action: &str,
    target: Option<&str>,
    ip: Option<&str>,
    success: bool,
    meta_json: Option<&str>,
) {
    let conn = lock(db);
    conn.execute(
        "INSERT INTO admin_audit (ts, token_prefix, action, target, ip, success, meta_json) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        params![now_ms(), token_prefix, action, target, ip, success as i32, meta_json],
    )
    .unwrap_or_else(|e| { tracing::warn!("审计写入失败: {e}"); 0 });
}

pub fn load_recent_audit(
    db: &Db,
    limit: usize,
    offset: usize,
    action_filter: Option<&str>,
) -> Vec<crate::admin::audit::AuditEntry> {
    let conn = lock(db);

    let (sql, use_filter) = match action_filter {
        Some(_) => (
            "SELECT id, ts, token_prefix, action, target, ip, success, meta_json \
             FROM admin_audit WHERE action = ?1 ORDER BY id DESC LIMIT ?2 OFFSET ?3",
            true,
        ),
        None => (
            "SELECT id, ts, token_prefix, action, target, ip, success, meta_json \
             FROM admin_audit ORDER BY id DESC LIMIT ?1 OFFSET ?2",
            false,
        ),
    };

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("审计查询失败: {e}");
            return Vec::new();
        }
    };

    let map_row = |row: &rusqlite::Row| -> rusqlite::Result<crate::admin::audit::AuditEntry> {
        Ok(crate::admin::audit::AuditEntry {
            id: row.get::<_, i64>(0)?,
            ts: row.get::<_, i64>(1)? as u64,
            token_prefix: row.get(2)?,
            action: row.get(3)?,
            target: row.get(4)?,
            ip: row.get(5)?,
            success: row.get::<_, i32>(6)? != 0,
            meta_json: row.get(7)?,
        })
    };

    let rows = if use_filter {
        stmt.query_map(
            params![action_filter.unwrap(), limit as i64, offset as i64],
            map_row,
        )
    } else {
        stmt.query_map(params![limit as i64, offset as i64], map_row)
    };

    rows.map(|r| r.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

pub fn count_audit(db: &Db, action_filter: Option<&str>) -> i64 {
    let conn = lock(db);
    match action_filter {
        Some(a) => conn
            .query_row(
                "SELECT COUNT(*) FROM admin_audit WHERE action = ?1",
                params![a],
                |row| row.get(0),
            )
            .unwrap_or(0),
        None => conn
            .query_row("SELECT COUNT(*) FROM admin_audit", [], |row| row.get(0))
            .unwrap_or(0),
    }
}

pub fn cleanup_old_audit(db: &Db, retention_days: u64) {
    let cutoff = now_ms() - (retention_days as i64 * 86_400_000);
    let conn = lock(db);
    conn.execute("DELETE FROM admin_audit WHERE ts < ?1", params![cutoff])
        .unwrap_or_else(|e| {
            tracing::warn!("审计清理失败: {e}");
            0
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrate_and_seed() {
        let db = open_memory();
        migrate(&db);

        let mut sites = HashMap::new();
        sites.insert(
            "pk_test".to_string(),
            SiteConfig {
                secret_key: "sk_1234567890123456".to_string(),
                diff: 18,
                origins: vec!["https://example.com".to_string()],
                argon2_m_cost: 19456,
                argon2_t_cost: 2,
                argon2_p_cost: 1,
                bind_token_to_ip: false,
                bind_token_to_ua: false,
                secret_key_hashed: false,
            },
        );
        seed_sites(&db, &sites);

        let loaded = load_sites(&db);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["pk_test"].diff, 18);
        assert_eq!(loaded["pk_test"].argon2_m_cost, 19456);
        assert_eq!(loaded["pk_test"].argon2_t_cost, 2);
    }

    #[test]
    fn site_crud() {
        let db = open_memory();
        migrate(&db);

        let site = SiteConfig {
            secret_key: "sk_abcdef1234567890".to_string(),
            diff: 16,
            origins: vec![],
            argon2_m_cost: 8192,
            argon2_t_cost: 3,
            argon2_p_cost: 1,
            bind_token_to_ip: false,
            bind_token_to_ua: false,
            secret_key_hashed: false,
        };
        insert_site(&db, "pk_new", &site);

        let loaded = load_sites(&db);
        assert_eq!(loaded["pk_new"].diff, 16);
        assert_eq!(loaded["pk_new"].argon2_m_cost, 8192);
        assert_eq!(loaded["pk_new"].argon2_t_cost, 3);

        update_site_fields(
            &db,
            "pk_new",
            Some(20),
            None,
            Some(32768),
            None,
            None,
            None,
            None,
        );
        let loaded = load_sites(&db);
        assert_eq!(loaded["pk_new"].diff, 20);
        assert_eq!(loaded["pk_new"].argon2_m_cost, 32768);
        assert_eq!(loaded["pk_new"].argon2_t_cost, 3);

        // v1.4.0 新增：开关身份绑定
        update_site_fields(
            &db,
            "pk_new",
            None,
            None,
            None,
            None,
            None,
            Some(true),
            Some(true),
        );
        let loaded = load_sites(&db);
        assert!(loaded["pk_new"].bind_token_to_ip);
        assert!(loaded["pk_new"].bind_token_to_ua);

        delete_site(&db, "pk_new");
        assert!(load_sites(&db).is_empty());
    }

    #[test]
    fn ip_list_crud() {
        let db = open_memory();
        migrate(&db);

        insert_ip_list(&db, "10.0.0.0/8", "blocked");
        insert_ip_list(&db, "127.0.0.1", "allowed");

        assert_eq!(load_ip_list(&db, "blocked"), vec!["10.0.0.0/8"]);
        assert_eq!(load_ip_list(&db, "allowed"), vec!["127.0.0.1"]);

        delete_ip_list(&db, "10.0.0.0/8", "blocked");
        assert!(load_ip_list(&db, "blocked").is_empty());
    }

    #[test]
    fn replay_nonce() {
        let db = open_memory();
        migrate(&db);

        assert!(mark_nonce_used(&db, "c1", "challenge", u64::MAX));
        assert!(!mark_nonce_used(&db, "c1", "challenge", u64::MAX));
    }

    #[test]
    fn request_log_roundtrip() {
        let db = open_memory();
        migrate(&db);

        let entry = LogEntry {
            timestamp: 1700000000000,
            ip: Some("1.2.3.4".parse().unwrap()),
            site_key: "pk_test".to_string(),
            nonce: 42,
            success: true,
            duration_ms: 5.5,
            error: None,
        };
        insert_log(&db, &entry);
        let logs = load_recent_logs(&db, 10);
        assert_eq!(logs.len(), 1);
        assert_eq!(logs[0].nonce, 42);
        assert!(logs[0].success);
    }

    #[test]
    fn server_secret_roundtrip() {
        let db = open_memory();
        migrate(&db);

        assert!(load_server_secret_32(&db, "manifest_signing_key").is_none());

        let seed = [0xbeu8; 32];
        save_server_secret_32(&db, "manifest_signing_key", &seed);
        assert_eq!(
            load_server_secret_32(&db, "manifest_signing_key"),
            Some(seed)
        );

        // 覆写
        let seed2 = [0xadu8; 32];
        save_server_secret_32(&db, "manifest_signing_key", &seed2);
        assert_eq!(
            load_server_secret_32(&db, "manifest_signing_key"),
            Some(seed2)
        );

        // 删除
        assert!(delete_server_secret(&db, "manifest_signing_key"));
        assert!(!delete_server_secret(&db, "manifest_signing_key"));
        assert!(load_server_secret_32(&db, "manifest_signing_key").is_none());
    }
}
