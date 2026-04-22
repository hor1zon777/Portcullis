use std::collections::HashMap;
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use serde::Deserialize;

use crate::risk::RiskConfig;

#[derive(Parser)]
#[command(name = "captcha-server", version, about = "PoW CAPTCHA 验证服务")]
pub struct Cli {
    #[arg(short, long)]
    pub config: Option<PathBuf>,

    #[arg(long)]
    pub bind: Option<String>,

    #[command(subcommand)]
    pub command: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    GenConfig,
    GenSecret,
    Healthcheck {
        #[arg(default_value = "127.0.0.1:8787")]
        addr: String,
    },
}

// ───────────── TOML 反序列化 ─────────────

#[derive(Debug, Deserialize)]
struct TomlConfig {
    server: Option<ServerSection>,
    #[serde(default)]
    sites: Vec<SiteSection>,
    #[serde(default)]
    risk: Option<RiskConfig>,
    #[serde(default)]
    admin: Option<AdminSection>,
}

#[derive(Debug, Deserialize)]
struct AdminSection {
    enabled: Option<bool>,
    token: Option<String>,
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

// ───────────── 运行时配置 ─────────────

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct SiteConfig {
    pub secret_key: String,
    pub diff: u8,
    #[serde(default)]
    pub origins: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct Config {
    pub secret: Vec<u8>,
    pub bind: String,
    pub sites: HashMap<String, SiteConfig>,
    pub token_ttl_secs: u64,
    pub challenge_ttl_secs: u64,
    pub risk: RiskConfig,
    pub admin_token: Option<String>,
    pub config_path: Option<PathBuf>,
}

impl Config {
    pub fn load(cli: &Cli) -> Self {
        let (toml_cfg, config_path) = load_toml(cli.config.as_ref());

        let (toml_server, toml_sites, toml_risk, toml_admin) = match toml_cfg {
            Some(t) => (t.server, t.sites, t.risk, t.admin),
            None => (None, Vec::new(), None, None),
        };

        let ts = toml_server.as_ref();

        let secret = std::env::var("CAPTCHA_SECRET")
            .ok()
            .or_else(|| ts.and_then(|s| s.secret.clone()));
        let secret = secret.expect(
            "缺少密钥。请设置 CAPTCHA_SECRET 环境变量或在 captcha.toml [server] 段设置 secret",
        );
        assert!(
            secret.len() >= 32,
            "密钥长度必须 >= 32 字节，当前 {} 字节",
            secret.len()
        );

        let bind = cli
            .bind
            .clone()
            .or_else(|| std::env::var("CAPTCHA_BIND").ok())
            .or_else(|| ts.and_then(|s| s.bind.clone()))
            .unwrap_or_else(|| "0.0.0.0:8787".to_string());

        let sites = if let Ok(sites_json) = std::env::var("CAPTCHA_SITES") {
            serde_json::from_str::<HashMap<String, SiteConfig>>(&sites_json)
                .expect("CAPTCHA_SITES 格式错误")
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
            tracing::warn!("未配置任何站点");
        }

        for (key, site) in &sites {
            assert!(
                site.secret_key.len() >= 16,
                "站点 '{}' 的 secret_key 长度必须 >= 16 字节",
                key
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

        let admin_token = std::env::var("CAPTCHA_ADMIN_TOKEN").ok().or_else(|| {
            toml_admin
                .as_ref()
                .filter(|a| a.enabled.unwrap_or(true))
                .and_then(|a| a.token.clone())
        });

        Self {
            secret: secret.into_bytes(),
            bind,
            sites,
            token_ttl_secs,
            challenge_ttl_secs,
            risk: toml_risk.unwrap_or_default(),
            admin_token,
            config_path,
        }
    }

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

fn load_toml(explicit_path: Option<&PathBuf>) -> (Option<TomlConfig>, Option<PathBuf>) {
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
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("读取配置文件 {} 失败: {e}", path.display()));
            let cfg: TomlConfig = toml::from_str(&content)
                .unwrap_or_else(|e| panic!("解析配置文件 {} 失败: {e}", path.display()));
            tracing::info!("已加载配置文件: {}", path.display());
            return (Some(cfg), Some(path));
        }
    }

    if let Some(p) = explicit_path {
        panic!("指定的配置文件不存在: {}", p.display());
    }

    (None, None)
}

pub fn print_config_template() {
    print!(
        r#"# PoW CAPTCHA 配置文件

[server]
bind = "0.0.0.0:8787"
secret = "CHANGE_ME_USE_captcha-server_gen-secret"
challenge_ttl_secs = 120
token_ttl_secs = 300

[[sites]]
key = "pk_example"
secret_key = "sk_example_change_me_min16"
diff = 18
origins = ["https://example.com"]

[risk]
dynamic_diff_enabled = true
dynamic_diff_max_increase = 4
window_size = 20
fail_rate_threshold = 0.7
blocked_ips = []
allowed_ips = ["127.0.0.1"]
"#
    );
}

pub fn print_gen_secret() {
    let mut buf = [0u8; 32];
    getrandom::getrandom(&mut buf).expect("随机数生成失败");
    let hex: String = buf.iter().map(|b| format!("{b:02x}")).collect();
    println!("{hex}");
}
