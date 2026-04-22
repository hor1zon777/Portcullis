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

    // 非阻塞日志 writer：避免高 QPS 下阻塞 tokio runtime
    let (non_blocking, _guard) = tracing_appender::non_blocking(std::io::stdout());
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "captcha_server=info,tower_http=info".into()),
        )
        .with_writer(non_blocking)
        .init();

    // 安装 Prometheus exporter
    let prom_handle = app_metrics::install();

    let cfg = config::Config::load(&cli);
    let bind = cfg.bind.clone();
    let site_count = cfg.sites.len();

    let app_state = AppState::new(cfg);

    // 后台清理 + 定期 gauge 更新
    let store = app_state.store.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(Duration::from_secs(30));
        loop {
            ticker.tick().await;
            store.cleanup_expired();
            app_metrics::register_store_metrics(&store);
        }
    });

    let limiter = IpRateLimiter::new(5, 20);
    let app = build_router(app_state, Some(limiter), Some(prom_handle));

    let addr: SocketAddr = bind.parse().expect("bind 地址格式错误");
    tracing::info!(
        "PoW 验证码服务启动：http://{} （{} 站点；/metrics 已启用）",
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
