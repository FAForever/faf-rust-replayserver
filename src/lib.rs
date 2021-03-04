use futures::join;
use server::server::run_server;

pub mod accept;
pub mod config;
pub mod database;
pub mod metrics;
pub mod replay;
pub mod server;
pub mod util;
pub mod worker_threads;

#[macro_use]
pub mod error;

use crate::config::InnerSettings;
use crate::server::signal::cancel_at_sigint;
use tokio_util::sync::CancellationToken;

async fn do_run_server() {
    let maybe_config = InnerSettings::from_env();
    let config = match maybe_config {
        Err(e) => {
            log::info!("Failed to load config: {}", e);
            return;
        }
        Ok(o) => o,
    };

    if !start_prometheus_server(&config) {
        return;
    }
    let shutdown_token = CancellationToken::new();
    let f1 = run_server(config, shutdown_token.clone());
    let f2 = cancel_at_sigint(shutdown_token);
    join!(f1, f2);
    /* Worker threads are joined once we drop the server. */
}

fn setup_process_exit_panic_hook() {
    let orig_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        orig_hook(panic_info);
        std::process::exit(1);
    }));
}

fn start_prometheus_server(config: &config::Settings) -> bool {
    let addr = format!("localhost:{}", config.server.prometheus_port);
    let parsed_addr = match addr.parse() {
        Err(e) => {
            log::error!("Failed to parse prometheus port: {}", e);
            return false;
        },
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

pub fn run() -> () {
    env_logger::init();
    setup_process_exit_panic_hook();
    let local_loop = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    local_loop.block_on(do_run_server());
}
