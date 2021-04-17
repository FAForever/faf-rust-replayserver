use std::thread;
use std::thread::JoinHandle;
use tokio::sync::mpsc::{channel, Sender};

use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::{config::Settings, replay::save::ReplaySaver, replay::Replays, server::connection::Connection};

#[derive(Clone)]
pub struct WorkerThreadWork {
    config: Settings,
    shutdown_token: CancellationToken,
    saver: ReplaySaver,
}

impl WorkerThreadWork {
    pub fn new(config: Settings, shutdown_token: CancellationToken, saver: ReplaySaver) -> Self {
        Self {
            config,
            shutdown_token,
            saver,
        }
    }

    pub fn run(self, s: Receiver<Connection>) {
        let mut replays = Replays::new(self.shutdown_token, self.config, self.saver);
        let wrapper = ReceiverStream::new(s);

        let local_loop = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        local_loop.block_on(async {
            replays.handle_connections_and_replays(wrapper).await;
        });
    }
}

pub struct WorkerThread {
    handle: Option<JoinHandle<()>>,
    channel: Sender<Connection>,
}

impl WorkerThread {
    pub fn new(work: WorkerThreadWork) -> Self {
        let (s, r) = channel(1);
        let handle = thread::spawn(move || work.run(r));
        Self {
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

    pub fn join(mut self) {
        drop(self.channel);
        self.handle.take().unwrap().join().unwrap();
    }
}

pub struct WorkerThreadPool {
    replay_workers: Vec<WorkerThread>,
}

impl WorkerThreadPool {
    pub fn new(work: WorkerThreadWork, count: u32) -> Self {
        let mut replay_workers = Vec::new();
        for _ in 0..count {
            let worker = WorkerThread::new(work.clone());
            replay_workers.push(worker);
        }
        Self { replay_workers }
    }

    /* Cancellable. */
    pub async fn assign_connection(&self, conn: Connection) {
        let conn_info = conn.get_header();
        let worker_to_pick = (conn_info.id % self.replay_workers.len() as u64) as usize;
        self.replay_workers[worker_to_pick].dispatch(conn).await;
    }

    pub fn join(self) {
        for worker in self.replay_workers.into_iter() {
            worker.join();
        }
    }
}
