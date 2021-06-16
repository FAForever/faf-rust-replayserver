use tokio::io::AsyncWriteExt;
use tokio_util::sync::CancellationToken;

use crate::{
    replay::streams::write_replay_stream, replay::streams::MReplayRef, server::connection::Connection,
    util::timeout::cancellable,
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
        if let Err(e) = self.do_send_replay_to_connection(c).await {
            log::info!("Replay send error: {}", e);
        };
    }

    async fn do_send_replay_to_connection(&self, c: &mut Connection) -> std::io::Result<()> {
        let written = write_replay_stream(&self.merged_replay, c).await?;
        c.flush().await?;
        c.shutdown().await?;
        log::debug!("{} ended, wrote {} bytes total", c, written);
        Ok(())
    }
}
