use faf_rust_replayserver::server::server::run_server;
use faf_rust_replayserver::util::process::{setup_process_exit_on_panic, wait_for_signals};
use tokio::join;

use faf_rust_replayserver::config::{InnerSettings, Settings};
use tokio_util::sync::CancellationToken;

const VERSION: &str = env!("CARGO_PKG_VERSION");

async fn do_run_server() {
    let maybe_config = InnerSettings::from_env();
    let config = match maybe_config {
        Err(e) => {
            log::info!("Failed to load config: {}", e);
            return;
        }
        Ok(o) => o,
    };

    log::info!(
        "Listening on port {}, prometheus server started on port {}.",
        config.server.port,
        config.server.prometheus_port
    );
    if !start_prometheus_server(&config) {
        return;
    }
    let shutdown_token = CancellationToken::new();
    let f1 = run_server(config, shutdown_token.clone());
    let f2 = async {
        wait_for_signals().await;
        log::debug!("Received a SIGINT or SIGTERM, shutting down");
        shutdown_token.cancel();
    };
    join!(f1, f2);
}

fn start_prometheus_server(config: &Settings) -> bool {
    let addr = format!("0.0.0.0:{}", config.server.prometheus_port);
    let parsed_addr = match addr.parse() {
        Err(e) => {
            log::error!("Failed to parse prometheus port: {}", e);
            return false;
        }
        Ok(a) => a,
    };
    match prometheus_exporter::start(parsed_addr) {
        Ok(..) => true,
        Err(e) => {
            log::debug!("Could not launch prometheus HTTP server: {}", e);
            false
        }
    }
}

fn configure_logging() {
    // sqlx logs all queries as info, which is a bit too verbose. Only log warnings and above,
    // we'll probably never need more, even for debugging.
    env_logger::Builder::from_default_env()
        .filter_module("sqlx", log::LevelFilter::Warn)
        .init();
}

pub fn main() {
    configure_logging();
    log::info!("Server version {}.", VERSION);
    setup_process_exit_on_panic();
    let local_loop = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    local_loop.block_on(do_run_server());
}
