use std::{cell::RefCell, rc::Rc};

use tokio::select;
use tokio_util::sync::CancellationToken;

use crate::{
    config::Settings,
    replay::streams::MReplayRef,
    replay::streams::{read_data, read_header, WriterReplay},
    server::connection::Connection,
    util::timeout::cancellable,
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
        let token = self
            .merge_strategy
            .borrow_mut()
            .replay_added(replay.clone());

        let read_from_connection = async {
            read_header(replay.clone(), c).await?;
            self.merge_strategy.borrow_mut().replay_header_added(token);
            select! {
                res = read_data(replay.clone(), c) => res,
                _ = self.stream_delay.track(&replay, &self.merge_strategy, token) => Ok(()),
            }
        };
        cancellable(read_from_connection, &self.shutdown_token).await;

        self.stream_delay
            .set_to_end(&replay, &self.merge_strategy, token);
        replay.borrow_mut().finish();
        self.merge_strategy.borrow_mut().replay_removed(token);
    }

    pub fn finalize(&self) {
        self.merge_strategy.borrow_mut().finish();
    }

    pub fn get_merged_replay(&self) -> MReplayRef {
        self.merge_strategy.borrow().get_merged_replay()
    }
}

// TODO merger tests.
