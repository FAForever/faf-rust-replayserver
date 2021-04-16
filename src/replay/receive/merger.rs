use std::{cell::RefCell, rc::Rc};

use tokio::join;
use tokio_util::sync::CancellationToken;

use crate::{
    config::Settings, replay::streams::read_from_connection, replay::streams::MReplayRef,
    replay::streams::WriterReplay, server::connection::Connection,
};

use super::{
    merge_strategy::MergeStrategy, quorum_merge_strategy::QuorumMergeStrategy,
    replay_delay::StreamDelay,
};

pub struct ReplayMerger {
    shutdown_token: CancellationToken,
    // Will be a boxed trait if ever needed.
    merge_strategy: RefCell<QuorumMergeStrategy>,
    stream_delay: StreamDelay,
}

impl ReplayMerger {
    pub fn new(shutdown_token: CancellationToken, config: Settings) -> Self {
        let stream_delay = StreamDelay::new(config.replay.delay_s, config.replay.update_interval_s);
        let merge_strategy = RefCell::new(QuorumMergeStrategy::new(
            config.replay.merge_quorum_size,
            config.replay.stream_comparison_distance_b,
        ));
        Self {
            shutdown_token,
            merge_strategy,
            stream_delay,
        }
    }

    pub async fn handle_connection(&self, c: &mut Connection) {
        let replay = Rc::new(RefCell::new(WriterReplay::new()));
        join! {
            read_from_connection(replay.clone(), c, self.shutdown_token.clone()),
            self.stream_delay.update_delayed_data_and_drive_merge_strategy(&replay, &self.merge_strategy),
        };
    }

    pub fn finalize(&self) {
        self.merge_strategy.borrow_mut().finish();
    }

    pub fn get_merged_replay(&self) -> MReplayRef {
        self.merge_strategy.borrow().get_merged_replay()
    }
}

// TODO merger tests.
