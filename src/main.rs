// For mocktopus
#![cfg_attr(test, feature(proc_macro_hygiene))]

use std::error::Error;
use accept::ConnectionProducer;
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
pub mod util;
#[macro_use] pub mod error;

use crate::server::server::Server;
use crate::config::Config;
use crate::server::signal::hold_until_signal;

async fn run_server() {
    let config = Config { worker_threads: 4, port: "7878".to_string() };
    let shutdown = StopSource::new();
    let token = shutdown.stop_token();
    let token_clone = token.clone();
    let producer = |consumer| {
        ConnectionProducer::new(format!("localhost:{}", config.port), token_clone.clone(), consumer)
    };
    let server = Server::new(&config, producer, token);
    let f1 = server.accept();
    let f2 = hold_until_signal(shutdown);
    join!(f1, f2);
}

fn main() -> Result<(), Box<dyn Error>> {
    env_logger::init();
    let mut pool = LocalPool::new();
    pool.spawner().spawn_local(run_server()).unwrap();
    pool.run();
    Ok(())
}
