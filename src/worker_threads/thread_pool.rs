use super::worker::ReplayThread;

use tokio::sync::mpsc::Receiver;
use tokio_stream::wrappers::ReceiverStream;
use tokio_util::sync::CancellationToken;

use crate::{
    config::Settings, replay::save::ReplaySaver, replay::Replays, server::connection::Connection,
};

pub struct ReplayThreadPool {
    replay_workers: Vec<ReplayThread>,
}

impl ReplayThreadPool {
    fn new(work: impl Fn(Receiver<Connection>) -> () + Send + 'static + Clone, count: u32) -> Self {
        let mut replay_workers = Vec::new();
        for _ in 0..count {
            let worker = ReplayThread::new(work.clone());
            replay_workers.push(worker);
        }
        Self { replay_workers }
    }

    pub fn from_context(context: ReplayThreadContext, count: u32) -> Self {
        Self::new(move |s| context.run_replays(s), count)
    }

    /* Cancellable. */
    pub async fn assign_connection(&self, conn: Connection) {
        let conn_info = conn.get_header();
        let worker_to_pick = (conn_info.id % self.replay_workers.len() as u64) as usize;
        self.replay_workers[worker_to_pick].dispatch(conn).await;
    }
}

#[derive(Clone)]
pub struct ReplayThreadContext {
    config: Settings,
    shutdown_token: CancellationToken,
    saver: ReplaySaver,
}

impl ReplayThreadContext {
    pub fn new(config: Settings, shutdown_token: CancellationToken, saver: ReplaySaver) -> Self {
        Self {
            config,
            shutdown_token,
            saver,
        }
    }

    pub fn run_replays(&self, s: Receiver<Connection>) {
        let c = self.clone();
        let mut replays = Replays::build(c.shutdown_token, c.config, c.saver);
        let wrapper = ReceiverStream::new(s);

        let local_loop = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        local_loop.block_on(async {
            replays.handle_connections_and_replays(wrapper).await;
        });
    }
}
