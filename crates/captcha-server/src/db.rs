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
    let conn = db.lock().unwrap();
    conn.execute_batch(
        "
        CREATE TABLE IF NOT EXISTS sites (
            key        TEXT PRIMARY KEY,
            secret_key TEXT    NOT NULL,
            diff       INTEGER NOT NULL DEFAULT 18,
            origins    TEXT    NOT NULL DEFAULT '[]',
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
        ",
    )
    .expect("数据库迁移失败");
}

// ──────── 种子数据 ────────

pub fn seed_sites(db: &Db, sites: &HashMap<String, SiteConfig>) {
    let conn = db.lock().unwrap();
    let now = now_ms();
    let mut stmt = conn
        .prepare("INSERT OR IGNORE INTO sites (key, secret_key, diff, origins, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)")
        .unwrap();
    for (key, site) in sites {
        let origins_json = serde_json::to_string(&site.origins).unwrap_or_else(|_| "[]".into());
        stmt.execute(params![
            key,
            site.secret_key,
            site.diff,
            origins_json,
            now,
            now
        ])
        .ok();
    }
}

pub fn seed_ip_lists(db: &Db, blocked: &[String], allowed: &[String]) {
    let conn = db.lock().unwrap();
    let now = now_ms();
    let mut stmt = conn
        .prepare("INSERT OR IGNORE INTO ip_lists (ip_or_cidr, list_type, created_at) VALUES (?1, ?2, ?3)")
        .unwrap();
    for ip in blocked {
        stmt.execute(params![ip, "blocked", now]).ok();
    }
    for ip in allowed {
        stmt.execute(params![ip, "allowed", now]).ok();
    }
}

// ──────── 加载 ────────

pub fn load_sites(db: &Db) -> HashMap<String, SiteConfig> {
    let conn = db.lock().unwrap();
    let mut stmt = conn
        .prepare("SELECT key, secret_key, diff, origins FROM sites")
        .unwrap();
    let rows = stmt
        .query_map([], |row| {
            let key: String = row.get(0)?;
            let secret_key: String = row.get(1)?;
            let diff: u8 = row.get(2)?;
            let origins_json: String = row.get(3)?;
            let origins: Vec<String> = serde_json::from_str(&origins_json).unwrap_or_default();
            Ok((
                key,
                SiteConfig {
                    secret_key,
                    diff,
                    origins,
                },
            ))
        })
        .unwrap();
    rows.filter_map(|r| r.ok()).collect()
}

pub fn load_ip_list(db: &Db, list_type: &str) -> Vec<String> {
    let conn = db.lock().unwrap();
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
    let conn = db.lock().unwrap();
    let now = now_ms();
    let origins_json = serde_json::to_string(&site.origins).unwrap_or_else(|_| "[]".into());
    conn.execute(
        "INSERT OR REPLACE INTO sites (key, secret_key, diff, origins, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![key, site.secret_key, site.diff, origins_json, now, now],
    )
    .ok();
}

pub fn update_site_fields(db: &Db, key: &str, diff: Option<u8>, origins: Option<&[String]>) {
    let conn = db.lock().unwrap();
    let now = now_ms();
    if let Some(d) = diff {
        conn.execute(
            "UPDATE sites SET diff = ?1, updated_at = ?2 WHERE key = ?3",
            params![d, now, key],
        )
        .ok();
    }
    if let Some(o) = origins {
        let json = serde_json::to_string(o).unwrap_or_else(|_| "[]".into());
        conn.execute(
            "UPDATE sites SET origins = ?1, updated_at = ?2 WHERE key = ?3",
            params![json, now, key],
        )
        .ok();
    }
}

pub fn delete_site(db: &Db, key: &str) {
    let conn = db.lock().unwrap();
    conn.execute("DELETE FROM sites WHERE key = ?1", params![key])
        .ok();
}

// ──────── IP 名单 ────────

pub fn insert_ip_list(db: &Db, ip: &str, list_type: &str) {
    let conn = db.lock().unwrap();
    conn.execute(
        "INSERT OR IGNORE INTO ip_lists (ip_or_cidr, list_type, created_at) VALUES (?1, ?2, ?3)",
        params![ip, list_type, now_ms()],
    )
    .ok();
}

pub fn delete_ip_list(db: &Db, ip: &str, list_type: &str) {
    let conn = db.lock().unwrap();
    conn.execute(
        "DELETE FROM ip_lists WHERE ip_or_cidr = ?1 AND list_type = ?2",
        params![ip, list_type],
    )
    .ok();
}

// ──────── 请求日志 ────────

pub fn insert_log(db: &Db, entry: &LogEntry) {
    let conn = db.lock().unwrap();
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
    .ok();
}

pub fn load_recent_logs(db: &Db, limit: usize) -> Vec<LogEntry> {
    let conn = db.lock().unwrap();
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
    let conn = db.lock().unwrap();
    conn.execute(
        "DELETE FROM request_log WHERE timestamp < ?1",
        params![cutoff],
    )
    .ok();
}

// ──────── 防重放 ────────

pub fn mark_nonce_used(db: &Db, id: &str, kind: &str, expires_ms: u64) -> bool {
    let conn = db.lock().unwrap();
    let result = conn.execute(
        "INSERT OR IGNORE INTO replay_nonces (id, kind, expires_ms) VALUES (?1, ?2, ?3)",
        params![id, kind, expires_ms as i64],
    );
    matches!(result, Ok(1))
}

pub fn cleanup_expired_nonces(db: &Db) {
    let now = now_ms();
    let conn = db.lock().unwrap();
    conn.execute(
        "DELETE FROM replay_nonces WHERE expires_ms <= ?1",
        params![now],
    )
    .ok();
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
            },
        );
        seed_sites(&db, &sites);

        let loaded = load_sites(&db);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded["pk_test"].diff, 18);
    }

    #[test]
    fn site_crud() {
        let db = open_memory();
        migrate(&db);

        let site = SiteConfig {
            secret_key: "sk_abcdef1234567890".to_string(),
            diff: 16,
            origins: vec![],
        };
        insert_site(&db, "pk_new", &site);

        let loaded = load_sites(&db);
        assert_eq!(loaded["pk_new"].diff, 16);

        update_site_fields(&db, "pk_new", Some(20), None);
        let loaded = load_sites(&db);
        assert_eq!(loaded["pk_new"].diff, 20);

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
}
