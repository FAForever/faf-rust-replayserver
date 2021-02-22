use tokio_util::sync::CancellationToken;

use crate::{
    replay::streams::MReplayRef, replay::streams::MergedReplayReader,
    server::connection::Connection, util::timeout::cancellable,
};

pub struct ReplaySender {
    merged_replay: MReplayRef,
    shutdown_token: CancellationToken,
}

impl ReplaySender {
    pub fn new(merged_replay: MReplayRef, shutdown_token: CancellationToken) -> Self {
        Self {
            merged_replay,
            shutdown_token,
        }
    }

    pub async fn handle_connection(&self, c: &mut Connection) {
        cancellable(self.send_replay_to_connection(c), &self.shutdown_token).await;
    }

    async fn send_replay_to_connection(&self, c: &mut Connection) {
        let mut reader = MergedReplayReader::new(self.merged_replay.clone());
        if let Err(..) = reader.write_to(c).await {
            todo!(); /* log */
        };
    }
}
