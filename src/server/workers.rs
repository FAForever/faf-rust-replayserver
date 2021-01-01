use std::thread;
use std::thread::JoinHandle;
use tokio::sync::mpsc::{Sender, channel, Receiver};
use std::option::Option;
use tokio_util::sync::CancellationToken;

use crate::server::connection::Connection;
use crate::replay::Replays;

type ThreadFn = fn(Receiver<Connection>, CancellationToken) -> ();

pub struct ReplayWorkerThread
{
    handle: Option<JoinHandle<()>>,
    channel: Sender<Connection>,
}

impl ReplayWorkerThread {
    pub fn new(work: ThreadFn, shutdown_token: CancellationToken) -> Self {
        let (s, r) = channel(1);
        let handle = thread::spawn(move || {
            work(r, shutdown_token)
        });
        ReplayWorkerThread {
            handle: Some(handle),
            channel: s,
        }
    }
    pub async fn dispatch(&self, c: Connection) {
        match self.channel.send(c).await {
            Ok(a) => a,
            _ => panic!("Could not dispatch a connection to a thread. Did it die?"),
        }
    }

    pub fn get_channel(&self) -> Sender<Connection> { self.channel.clone() }
}

impl Drop for ReplayWorkerThread {
    fn drop(&mut self) {
        self.handle.take().unwrap().join().unwrap();    /* TODO log */
    }
}

/* TODO move elsewhere */
pub fn dummy_work(streams: Receiver<Connection>, shutdown_token: CancellationToken)
{
    let local_loop = tokio::runtime::Builder::new_current_thread().build().unwrap();
    local_loop.block_on(do_work(streams, shutdown_token));
}

async fn do_work(streams: Receiver<Connection>, shutdown_token: CancellationToken)
{
    let mut replays = Replays::new(shutdown_token);
    replays.handle_connections(streams).await;
}


pub struct ReplayThreadPool
{
    replay_workers: Vec<ReplayWorkerThread>,
}

impl ReplayThreadPool {
    pub fn new(work: ThreadFn, count: u32, shutdown_token: CancellationToken) -> Self {
        let mut replay_workers = Vec::new();
        for _ in 0..count {
            let worker = ReplayWorkerThread::new(work, shutdown_token.clone());
            replay_workers.push(worker);
        }
        Self { replay_workers }
    }

    /* Cancellable. */
    pub async fn assign_connection(&self, conn: Connection) {
        let conn_info = conn.get_header();
        assert!(conn_info.is_some());
        let conn_info = conn_info.unwrap();
        let worker_to_pick = (conn_info.id % self.replay_workers.len() as u64) as usize;
        self.replay_workers[worker_to_pick].dispatch(conn).await;
    }
}
