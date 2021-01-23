use std::{cell::RefCell, rc::Rc, time::Duration};

use tokio::join;
use tokio_util::sync::CancellationToken;

use crate::{server::connection::Connection, replay::streams::read_from_connection, replay::streams::WriterReplay, util::empty_counter::EmptyCounter};

use super::{merge_strategy::NullMergeStrategy, merge_strategy::track_replay, replay_delay::StreamDelay, merge_strategy::MReplayRef, merge_strategy::MergeStrategy};

pub struct ReplayMerger {
    shutdown_token: CancellationToken,
    // Will be a boxed trait if ever needed.
    merge_strategy: RefCell<NullMergeStrategy>,
    stream_delay: StreamDelay,
    no_connections_counter: EmptyCounter,
}

impl ReplayMerger {
    pub fn new(shutdown_token: CancellationToken) -> Self {
        // FIXME configure / inject
        let stream_delay = StreamDelay::new(300, 1000);
        let merge_strategy = RefCell::new(NullMergeStrategy {});
        let no_connections_counter = EmptyCounter::new();
        Self {shutdown_token, merge_strategy, stream_delay, no_connections_counter }
    }
    pub async fn lifetime(&self) {
        // FIXME make configurable
        self.no_connections_counter.wait_until_empty_for(Duration::from_secs(30)).await;
    }

    pub async fn handle_connection(&self, c: &mut Connection) {
        let _guard = self.no_connections_counter.guard();
        let replay = Rc::new(RefCell::new(WriterReplay::new()));
        join! {
            read_from_connection(replay.clone(), c, self.shutdown_token.clone()),
            track_replay(&self.merge_strategy, &self.stream_delay, replay),
        };
    }

    pub fn get_merged_replay(&self) -> MReplayRef {
        self.merge_strategy.borrow().get_merged_replay()
    }
}
