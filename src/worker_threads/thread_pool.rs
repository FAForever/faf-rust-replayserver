use super::worker::{ReplayWorkerThread, ThreadFn};
use crate::server::connection::Connection;

pub struct ReplayThreadPool {
    replay_workers: Vec<ReplayWorkerThread>,
}

impl ReplayThreadPool {
    // Funky type so that we can clone work closures for each thread.
    pub fn new(work: impl Fn() -> ThreadFn, count: u32) -> Self {
        let mut replay_workers = Vec::new();
        for _ in 0..count {
            let worker = ReplayWorkerThread::new(work());
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
}
