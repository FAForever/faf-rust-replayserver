use std::error::Error;
use stop_token::StopSource;
use futures::{
    task::LocalSpawnExt,
    executor::LocalPool,
    join,
};

pub mod accept;
pub mod config;
pub mod server;
pub mod replay;
pub mod error;

use crate::server::server::Server;
use crate::config::Config;
use crate::server::signal::hold_until_signal;
use crate::accept::ConnectionAcceptor;

async fn run_server() {
    let config = Config { worker_threads: 4, port: "7878".to_string() };
    let shutdown = StopSource::new();
    let token = shutdown.stop_token();
    let (acceptor, connection_stream) = ConnectionAcceptor::new(format!("localhost:{}", config.port), &token);
    let mut server = Server::new(&config, acceptor, connection_stream);
    let f1 = server.accept();
    let f2 = hold_until_signal(shutdown);
    join!(f1, f2);
    server.shutdown();
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let mut pool = LocalPool::new();
    pool.spawner().spawn_local(run_server()).unwrap();
    pool.run();
    Ok(())
}
