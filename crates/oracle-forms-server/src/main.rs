mod assets;
mod oracle_preview;
mod routes;
mod runtime_sessions;
mod security;
mod service;

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};
use crate::routes::app_router_with_security;
use crate::security::SecurityConfig;
use crate::service::FormsService;

fn env_flag(name: &str) -> bool {
    matches!(
        std::env::var(name).map(|v| v.trim().to_ascii_lowercase()),
        Ok(v) if v == "1" || v == "true" || v == "yes"
    )
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "oracle_forms_server=info,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let cache_dir = std::env::temp_dir().join("forms-viewer-cache");
    let allow_env_credentials = env_flag("OFW_SERVER_ENV_CREDENTIALS");
    let service = FormsService::new(cache_dir).with_env_credentials(allow_env_credentials);

    let port: u16 = std::env::var("PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(7070);

    // Loopback only by default. Exposing the server on the network requires an explicit
    // OFW_HOST, and the hostnames clients will use must be listed in OFW_ALLOWED_HOSTS.
    let host: IpAddr = match std::env::var("OFW_HOST") {
        Ok(h) => h
            .parse()
            .map_err(|e| format!("Invalid OFW_HOST '{}': {}", h, e))?,
        Err(_) => IpAddr::V4(Ipv4Addr::LOCALHOST),
    };
    let security_cfg = SecurityConfig::from_env();

    // Expire idle form sessions.
    let reaper = service.sessions.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            tick.tick().await;
            reaper.reap_idle();
        }
    });

    let app = app_router_with_security(service, security_cfg.clone())
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::new(host, port);
    println!("============================================================");
    println!("  Oracle Forms Workbench (Rust / Axum Server)");
    println!("  Listening on http://{}", addr);
    println!("============================================================");
    if !host.is_loopback() {
        eprintln!(
            "WARNING: listening on non-loopback address {}. Anyone who can reach this address can read \
             files this server can open and use its database connections. Only accepted Host names: {:?} \
             (set OFW_ALLOWED_HOSTS to add the name clients use).",
            host,
            security_cfg.allowed_hosts()
        );
    }
    if allow_env_credentials {
        eprintln!(
            "NOTICE: OFW_SERVER_ENV_CREDENTIALS is on: database requests without credentials will use \
             OFW_DB_USER / OFW_DB_PASSWORD from this server's environment."
        );
    }

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
