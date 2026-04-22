use std::net::SocketAddr;
use std::time::Duration;

use captcha_server::config::{self, Cli, Commands};
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
        }
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "captcha_server=info,tower_http=info".into()),
        )
        .init();

    let cfg = config::Config::load(&cli);
    let bind = cfg.bind.clone();
    let site_count = cfg.sites.len();

    let app_state = AppState::new(cfg);

    // 后台过期清理
    let store = app_state.store.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(60));
        loop {
            ticker.tick().await;
            let removed = store.cleanup_expired();
            if removed > 0 {
                tracing::debug!("清理 {} 条过期记录，剩余 {}", removed, store.len());
            }
        }
    });

    // IP 限流：突发 20，持续 5 req/s
    let limiter = IpRateLimiter::new(5, 20);
    let app = build_router(app_state, Some(limiter));

    let addr: SocketAddr = bind.parse().expect("bind 地址格式错误");
    tracing::info!(
        "PoW 验证码服务启动：http://{} （{} 站点，IP 限流 5 req/s burst 20）",
        addr,
        site_count
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
