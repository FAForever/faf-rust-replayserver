use accept::ConnectionProducer;
use database::database::Database;
use futures::join;

pub mod accept;
pub mod config;
pub mod database;
pub mod replay;
pub mod server;
pub mod util;
pub mod worker_threads;

#[macro_use]
pub mod error;

use crate::config::Settings;
use crate::server::server::Server;
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
    let database = Database::new(&config.database);
    let server = Server::new(config, producer, database, shutdown_token.clone());
    let f1 = server.accept();
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

pub fn run() -> () {
    env_logger::init();
    setup_process_exit_panic_hook();
    let local_loop = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();
    local_loop.block_on(run_server());
}
