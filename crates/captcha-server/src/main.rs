use std::net::SocketAddr;
use std::time::Duration;

use captcha_server::config::{self, Cli, Commands};
use captcha_server::metrics as app_metrics;
use captcha_server::rate_limit::IpRateLimiter;
use captcha_server::{build_router, state::AppState};
use clap::Parser;

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    if let Some(cmd) = &cli.command {
        match cmd {
            Commands::GenConfig => {
                config::print_config_template();
                return;
            }
            Commands::GenSecret => {
                config::print_gen_secret();
                return;
            }
            Commands::Healthcheck { addr } => {
                run_healthcheck(addr);
                return;
            }
        }
    }

    let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "captcha_server=info,tower_http=info".into()),
        )
        .with_writer(non_blocking)
        .init();

    let prom_handle = app_metrics::install();

    let mut cfg = config::Config::load(&cli);
    let bind = cfg.bind.clone();
    let config_path = cfg.config_path.clone();

    // 初始化 SQLite
    let db = captcha_server::db::open(&cfg.db_path);
    captcha_server::db::migrate(&db);

    // Seed TOML 数据到 DB（仅首次 INSERT OR IGNORE）
    captcha_server::db::seed_sites(&db, &cfg.sites);
    captcha_server::db::seed_ip_lists(&db, &cfg.risk.blocked_ips, &cfg.risk.allowed_ips);

    // 从 DB 加载（DB 为 source of truth）
    cfg.sites = captcha_server::db::load_sites(&db);
    cfg.risk.blocked_ips = captcha_server::db::load_ip_list(&db, "blocked");
    cfg.risk.allowed_ips = captcha_server::db::load_ip_list(&db, "allowed");

    let site_count = cfg.sites.len();
    tracing::info!("SQLite 已初始化：{}", cfg.db_path.display());

    let app_state = AppState::new(cfg, db.clone());

    // 后台任务：store 清理 + risk 清理 + 指标采集 + DB 清理 + 配置热重载
    let bg_state = app_state.clone();
    let bg_config_path = config_path.clone();
    let bg_db = db.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(30));
        let mut last_mtime: Option<std::time::SystemTime> = None;
        loop {
            ticker.tick().await;

            // store 清理
            bg_state.store.cleanup_expired();
            app_metrics::register_store_metrics(&bg_state.store);

            // DB 清理（过期日志 + 过期 nonce）
            {
                let db = bg_db.clone();
                let _ = tokio::task::spawn_blocking(move || {
                    captcha_server::db::cleanup_old_logs(&db, 7);
                    captcha_server::db::cleanup_expired_nonces(&db);
                })
                .await;
            }

            // risk tracker 清理
            {
                let risk = bg_state.risk.read().await;
                let removed = risk.cleanup_stale();
                if removed > 0 {
                    tracing::debug!("清理 {} 条过期 IP 记录", removed);
                }
            }

            // 配置热重载：检查 mtime
            if let Some(ref path) = bg_config_path {
                if let Ok(meta) = std::fs::metadata(path) {
                    if let Ok(mtime) = meta.modified() {
                        let changed = last_mtime.map(|prev| mtime != prev).unwrap_or(false);
                        last_mtime = Some(mtime);
                        if changed {
                            tracing::info!("检测到配置文件变更，开始热重载...");
                            let new_cli = Cli {
                                config: Some(path.clone()),
                                bind: None,
                                command: None,
                            };
                            let new_cfg = config::Config::load(&new_cli);
                            bg_state.reload_config(new_cfg).await;
                        }
                    }
                }
            }
        }
    });

    let limiter = IpRateLimiter::new(5, 20);
    let app = build_router(app_state, Some(limiter), Some(prom_handle));

    let addr: SocketAddr = bind.parse().expect("bind 地址格式错误");
    tracing::info!(
        "PoW 验证码服务启动：http://{} （{} 站点 | /metrics | 配置热重载{}）",
        addr,
        site_count,
        if config_path.is_some() {
            "已启用"
        } else {
            "未启用（无配置文件）"
        }
    );

    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .expect("绑定端口失败");
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .expect("服务运行异常");
}

fn run_healthcheck(addr: &str) {
    use std::net::{SocketAddr, TcpStream};
    use std::time::Duration;

    let parsed: SocketAddr = match addr.parse() {
        Ok(a) => a,
        Err(_) => {
            eprintln!("无效地址: {addr}");
            std::process::exit(2);
        }
    };
    match TcpStream::connect_timeout(&parsed, Duration::from_secs(3)) {
        Ok(_) => std::process::exit(0),
        Err(e) => {
            eprintln!("健康检查失败: {e}");
            std::process::exit(1);
        }
    }
}
