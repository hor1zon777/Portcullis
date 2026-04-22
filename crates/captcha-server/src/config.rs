use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Deserialize;

/// PoW 验证码服务。
#[derive(Parser)]
#[command(name = "captcha-server", version, about = "PoW CAPTCHA 验证服务")]
pub struct Cli {
    /// 配置文件路径（默认查找 ./captcha.toml 或 /etc/captcha/captcha.toml）
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    /// 覆盖监听地址
    #[arg(long)]
    pub bind: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// 生成配置文件模板
    GenConfig,
    /// 生成 32 字节随机密钥（十六进制）
    GenSecret,
    /// 健康检查（TCP 探测指定地址，Docker healthcheck 用）
    Healthcheck {
        /// 探测地址，默认 127.0.0.1:8787
        #[arg(default_value = "127.0.0.1:8787")]
        addr: String,
    },
}

/// TOML 配置的反序列化结构。
#[derive(Debug, Deserialize)]
struct TomlConfig {
    server: Option<ServerSection>,
    #[serde(default)]
    sites: Vec<SiteSection>,
}

#[derive(Debug, Deserialize)]
struct ServerSection {
    bind: Option<String>,
    secret: Option<String>,
    challenge_ttl_secs: Option<u64>,
    token_ttl_secs: Option<u64>,
}

#[derive(Debug, Deserialize)]
struct SiteSection {
    key: String,
    secret_key: String,
    diff: u8,
    #[serde(default)]
    origins: Vec<String>,
}

/// 单个站点的配置。
#[derive(Debug, Clone, Deserialize)]
pub struct SiteConfig {
    pub secret_key: String,
    pub diff: u8,
    #[serde(default)]
    pub origins: Vec<String>,
}

/// 运行时全局配置。
#[derive(Debug, Clone)]
pub struct Config {
    pub secret: Vec<u8>,
    pub bind: String,
    pub sites: HashMap<String, SiteConfig>,
    pub token_ttl_secs: u64,
    pub challenge_ttl_secs: u64,
}

impl Config {
    /// 从 TOML 文件 + 环境变量 + CLI 参数加载配置。
    /// 优先级：CLI > 环境变量 > TOML > 默认值。
    pub fn load(cli: &Cli) -> Self {
        let toml_cfg = load_toml(cli.config.as_ref());

        let (toml_server, toml_sites) = match toml_cfg {
            Some(t) => (t.server, t.sites),
            None => (None, Vec::new()),
        };

        let ts = toml_server.as_ref();

        // secret: env > toml
        let secret = std::env::var("CAPTCHA_SECRET").ok().or_else(|| {
            ts.and_then(|s| s.secret.clone())
        });
        let secret = secret.expect(
            "缺少密钥。请设置 CAPTCHA_SECRET 环境变量或在 captcha.toml [server] 段设置 secret"
        );
        assert!(
            secret.len() >= 32,
            "密钥长度必须 >= 32 字节，当前 {} 字节。运行 `captcha-server gen-secret` 生成。",
            secret.len()
        );

        // bind: CLI > env > toml > default
        let bind = cli
            .bind
            .clone()
            .or_else(|| std::env::var("CAPTCHA_BIND").ok())
            .or_else(|| ts.and_then(|s| s.bind.clone()))
            .unwrap_or_else(|| "0.0.0.0:8787".to_string());

        // sites: env > toml
        let sites = if let Ok(sites_json) = std::env::var("CAPTCHA_SITES") {
            serde_json::from_str::<HashMap<String, SiteConfig>>(&sites_json)
                .expect("CAPTCHA_SITES 格式错误，应为 JSON 对象")
        } else {
            let mut map = HashMap::new();
            for site in toml_sites {
                map.insert(
                    site.key,
                    SiteConfig {
                        secret_key: site.secret_key,
                        diff: site.diff,
                        origins: site.origins,
                    },
                );
            }
            map
        };

        if sites.is_empty() {
            tracing::warn!("未配置任何站点（sites 为空）。请在 captcha.toml 添加 [[sites]] 段或设置 CAPTCHA_SITES 环境变量。");
        }

        // 校验 secret_key 最小长度
        for (key, site) in &sites {
            assert!(
                site.secret_key.len() >= 16,
                "站点 '{}' 的 secret_key 长度必须 >= 16 字节，当前 {} 字节",
                key,
                site.secret_key.len()
            );
        }

        let challenge_ttl_secs = std::env::var("CAPTCHA_CHALLENGE_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .or_else(|| ts.and_then(|s| s.challenge_ttl_secs))
            .unwrap_or(120);

        let token_ttl_secs = std::env::var("CAPTCHA_TOKEN_TTL_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .or_else(|| ts.and_then(|s| s.token_ttl_secs))
            .unwrap_or(300);

        Self {
            secret: secret.into_bytes(),
            bind,
            sites,
            token_ttl_secs,
            challenge_ttl_secs,
        }
    }

    /// 测试专用：从纯环境变量加载。
    #[cfg(test)]
    pub fn from_env() -> Self {
        Self::load(&Cli {
            config: None,
            bind: None,
            command: None,
        })
    }

    pub fn get_site(&self, site_key: &str) -> Option<&SiteConfig> {
        self.sites.get(site_key)
    }
}

fn load_toml(explicit_path: Option<&PathBuf>) -> Option<TomlConfig> {
    let candidates: Vec<PathBuf> = if let Some(p) = explicit_path {
        vec![p.clone()]
    } else {
        vec![
            PathBuf::from("captcha.toml"),
            PathBuf::from("/etc/captcha/captcha.toml"),
        ]
    };

    for path in candidates {
        if path.exists() {
            let content = std::fs::read_to_string(&path).unwrap_or_else(|e| {
                panic!("读取配置文件 {} 失败: {e}", path.display())
            });
            let cfg: TomlConfig = toml::from_str(&content).unwrap_or_else(|e| {
                panic!("解析配置文件 {} 失败: {e}", path.display())
            });
            tracing::info!("已加载配置文件: {}", path.display());
            return Some(cfg);
        }
    }

    if let Some(p) = explicit_path {
        panic!("指定的配置文件不存在: {}", p.display());
    }

    None
}

pub fn print_config_template() {
    print!(
        r#"# PoW CAPTCHA 配置文件

[server]
bind = "0.0.0.0:8787"
# 密钥至少 32 字节，运行 `captcha-server gen-secret` 生成
secret = "CHANGE_ME_USE_captcha-server_gen-secret"
challenge_ttl_secs = 120
token_ttl_secs = 300

[[sites]]
key = "pk_example"
secret_key = "sk_example_change_me"
diff = 18
origins = ["https://example.com"]

# 可添加多个站点
# [[sites]]
# key = "pk_mobile"
# secret_key = "sk_mobile_change_me"
# diff = 14
# origins = ["https://m.example.com"]
"#
    );
}

pub fn print_gen_secret() {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).expect("随机数生成失败");
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    println!("{hex}");
}
