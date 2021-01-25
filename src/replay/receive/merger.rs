use std::{cell::RefCell, rc::Rc};

use tokio::join;
use tokio_util::sync::CancellationToken;

use crate::{server::connection::Connection, replay::streams::read_from_connection, replay::streams::WriterReplay, replay::streams::MReplayRef};

use super::{replay_delay::StreamDelay, merge_strategy::MergeStrategy, quorum_merge_strategy::QuorumMergeStrategy};

pub struct ReplayMerger {
    shutdown_token: CancellationToken,
    // Will be a boxed trait if ever needed.
    merge_strategy: RefCell<QuorumMergeStrategy>,
    stream_delay: StreamDelay,
}

impl ReplayMerger {
    pub fn new(shutdown_token: CancellationToken) -> Self {
        // FIXME configure / inject
        let stream_delay = StreamDelay::new(300, 1000);
        let merge_strategy = RefCell::new(QuorumMergeStrategy::new());
        Self {shutdown_token, merge_strategy, stream_delay }
    }

    pub async fn handle_connection(&self, c: &mut Connection) {
        let replay = Rc::new(RefCell::new(WriterReplay::new()));
        join! {
            read_from_connection(replay.clone(), c, self.shutdown_token.clone()),
            self.stream_delay.update_delayed_data_and_drive_merge_strategy(replay, &self.merge_strategy),
        };
    }

    pub fn finalize(&self) {
        self.merge_strategy.borrow_mut().finish();
    }

    pub fn get_merged_replay(&self) -> MReplayRef {
        self.merge_strategy.borrow().get_merged_replay()
    }
}
