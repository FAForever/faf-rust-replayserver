use accept::ConnectionProducer;
use futures::join;

pub mod config;
pub mod util;
pub mod server;
pub mod worker_threads;
pub mod accept;
pub mod replay;

#[macro_use] pub mod error;

use crate::server::server::Server;
use crate::config::Settings;
use crate::server::signal::cancel_at_sigint;
use tokio_util::sync::CancellationToken;

async fn run_server() {
    let maybe_config = Settings::from_env();
    if let Err(e) = maybe_config {
        log::info!("Failed to load config: {}", e);
        return;
    }
    let config = maybe_config.unwrap();

    let shutdown_token = CancellationToken::new();
    let producer = ConnectionProducer::new(format!("localhost:{}", config.server.port));
    let server = Server::new(&config, producer, shutdown_token.clone());
    let f1 = server.accept();
    let f2 = cancel_at_sigint(shutdown_token);
    join!(f1, f2);
    /* Worker threads are joined once we drop the server. */
}

pub fn run() -> () {
    env_logger::init();
    let local_loop = tokio::runtime::Builder::new_current_thread().build().unwrap();
    local_loop.block_on(run_server());
}
