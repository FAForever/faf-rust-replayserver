use std::thread;
use std::thread::JoinHandle;
use tokio::sync::mpsc::{channel, Sender};

use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::{config::Settings, replay::save::ReplaySaver, replay::Replays, server::connection::Connection};

fn handle_replays(config: Settings, shutdown_token: CancellationToken, saver: ReplaySaver) -> impl FnOnce(Receiver<Connection>) + Clone + Send {
   move |s| {
        let mut replays = Replays::new(shutdown_token, config, saver);
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

struct WorkerThread {
    handle: Option<JoinHandle<()>>,
    channel: Sender<Connection>,
}

impl WorkerThread {
    fn new(work: impl FnOnce(Receiver<Connection>) + Send + 'static) -> Self {
        let (s, r) = channel(1);
        let handle = thread::spawn(move || work(r));
        Self {
            handle: Some(handle),
            channel: s,
        }
    }

    async fn dispatch(&self, c: Connection) {
        match self.channel.send(c).await {
            Ok(a) => a,
            _ => panic!("Could not dispatch a connection to a thread. Did it die?"),
        }
    }

    fn join(mut self) {
        drop(self.channel);
        self.handle.take().unwrap().join().unwrap();
    }
}

pub struct ReplayRunner {
    replay_workers: Vec<WorkerThread>,
}

// Distributes replay IDs among worker threads and gives them connections to handle.
impl ReplayRunner {
    pub fn new(config: Settings, shutdown_token: CancellationToken, saver: ReplaySaver) -> Self {
        let count = config.server.worker_threads;
        let handle_some_replays = handle_replays(config, shutdown_token, saver);
        let mut replay_workers = Vec::new();
        for _ in 0..count {
            let worker = WorkerThread::new(handle_some_replays.clone());
            replay_workers.push(worker);
        }
        Self { replay_workers }
    }
    
    pub async fn dispatch_connection(&self, conn: Connection) {
        let conn_info = conn.get_header();
        let worker_to_pick = (conn_info.id % self.replay_workers.len() as u64) as usize;
        self.replay_workers[worker_to_pick].dispatch(conn).await;
    }

    pub fn shutdown(self) {
        for worker in self.replay_workers.into_iter() {
            worker.join();
        }
    }
}
