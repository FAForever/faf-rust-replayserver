use async_std::sync::{channel, Sender, Receiver};
use std::thread;
use std::thread::JoinHandle;
use stop_token::StopToken;
use futures::{
    executor::LocalPool,
    task::SpawnExt,
};
use std::option::Option;

use crate::server::connection::Connection;
use crate::replay::Replays;

type ThreadFn = fn(Receiver<Connection>, StopToken) -> ();

pub struct ReplayWorkerThread
{
    handle: Option<JoinHandle<()>>,
    channel: Sender<Connection>,
}

impl ReplayWorkerThread {
    pub fn new(work: ThreadFn, stop_token: StopToken) -> Self {
        let (s, r) = channel(1);
        let handle = thread::spawn(move || {
            work(r, stop_token)
        });
        ReplayWorkerThread {
            handle: Some(handle),
            channel: s,
        }
    }
    pub async fn dispatch(&self, c: Connection) {
        self.channel.send(c).await
    }

    pub fn get_channel(&self) -> Sender<Connection> { self.channel.clone() }
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


pub struct ReplayThreadPool
{
    replay_workers: Vec<ReplayWorkerThread>,
}

impl ReplayThreadPool {
    pub fn new(work: ThreadFn, count: u32, stop_token: StopToken) -> Self {
        let mut replay_workers = Vec::new();
        for _ in 0..count {
            let worker = ReplayWorkerThread::new(work, stop_token.clone());
            replay_workers.push(worker);
        }
        Self { replay_workers }
    }

    pub async fn assign_connection(&self, conn: Connection) {
        let conn_info = conn.get_header().as_ref();
        assert!(conn_info.is_some());
        let conn_info = conn_info.unwrap();
        let worker_to_pick = (conn_info.id % self.replay_workers.len() as u64) as usize;
        self.replay_workers[worker_to_pick].dispatch(conn).await;
    }
}
