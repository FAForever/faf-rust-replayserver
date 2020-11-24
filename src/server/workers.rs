use async_std::sync::{channel, Sender, Receiver};
use std::thread;
use std::thread::JoinHandle;
use stop_token::StopToken;
use futures::{
    executor::LocalPool,
    task::SpawnExt,
};
use std::option::Option;
use stop_token::StopSource;

use crate::server::connection::Connection;
use crate::replay::Replays;

pub type ThreadFn = fn(Receiver<Connection>, StopToken) -> ();

pub struct ReplayWorkerThread
{
    handle: Option<JoinHandle<()>>,
    channel: Sender<Connection>,
    shutdown_token: Option<StopSource>,
}

impl ReplayWorkerThread {
    pub fn new(work: ThreadFn) -> Self {
        let (s, r) = channel(1);
        let shutdown_token = StopSource::new();
        let stop_token = shutdown_token.stop_token();
        let handle = thread::spawn(move || {
            work(r, stop_token)
        });
        ReplayWorkerThread {
            handle: Some(handle),
            channel: s,
            shutdown_token: Some(shutdown_token)
        }
    }
    pub fn get_channel(&self) -> Sender<Connection> { self.channel.clone() }
    pub fn shutdown(&mut self) { self.shutdown_token = None}
}

impl Drop for ReplayWorkerThread {
    fn drop(&mut self) {
        self.handle.take().unwrap().join().unwrap();    /* TODO log */
    }
}

/* TODO move elsewhere */
pub fn dummy_work(streams: Receiver<Connection>, shutdown_token: StopToken)
{
    let mut pool = LocalPool::new();
    pool.spawner().spawn(do_work(streams, shutdown_token)).unwrap();
    pool.run()
}

async fn do_work(streams: Receiver<Connection>, shutdown_token: StopToken)
{
    let mut replays = Replays::new(shutdown_token, streams);
    replays.lifetime().await;
}
