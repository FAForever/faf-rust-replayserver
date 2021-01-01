use accept::ConnectionProducer;
use futures::join;

pub mod accept;
pub mod config;
pub mod server;
pub mod replay;
pub mod async_utils;
#[macro_use] pub mod error;

use crate::server::server::Server;
use crate::config::Config;
use crate::server::signal::hold_until_signal;
use tokio_util::sync::CancellationToken;

async fn run_server() {
    let config = Config { worker_threads: 4, port: "7878".to_string() };
    let shutdown_token = CancellationToken::new();
    let producer = ConnectionProducer::new(format!("localhost:{}", config.port));
    let server = Server::new(&config, producer, shutdown_token.clone());
    let f1 = server.accept();
    let f2 = hold_until_signal(shutdown_token);
    join!(f1, f2);
    /* Worker threads are joined once we drop the server. */
}

fn main() -> () {
    env_logger::init();
    let local_loop = tokio::runtime::Builder::new_current_thread().build().unwrap();
    local_loop.block_on(run_server());
}
