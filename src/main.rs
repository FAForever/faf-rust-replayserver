use async_std::net::TcpStream;
use futures::executor::LocalPool;
use async_channel::Receiver;
use futures::task::LocalSpawnExt;
use std::error::Error;

mod server;
use crate::server::workers::{WorkerThread, create_worker_thread};
use crate::server::server as s;

fn create_stream_thread_pool(count: u32, work: fn(Receiver<TcpStream>) -> ()) -> Vec<WorkerThread<TcpStream>> {
    let mut v = Vec::new();
    for _ in 0..count {
        v.push(create_worker_thread(work));
    }
    v
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut pool = LocalPool::new();
    let workers = create_stream_thread_pool(4, s::thread_accept_connections);
    pool.spawner().spawn_local(s::accept_connections(workers))?;
    pool.run();
    Ok(())
}
