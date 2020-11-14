use futures::executor::LocalPool;
use futures::task::SpawnExt;
use std::error::Error;
mod server;

fn main() -> Result<(), Box<dyn Error>> {
    let mut pool = LocalPool::new();
    pool.spawner().spawn(server::accept_connections())?;
    pool.run();
    Ok(())
}
