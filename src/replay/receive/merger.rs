use std::{cell::RefCell, rc::Rc};

use tokio::join;
use tokio_util::sync::CancellationToken;

use crate::{server::connection::Connection, replay::streams::read_from_connection, replay::streams::WriterReplay};

use super::{merge_strategy::NullMergeStrategy, merge_strategy::track_replay, replay_delay::StreamDelay};

pub struct ReplayMerger {
    shutdown_token: CancellationToken,
    // Will be a boxed trait if ever needed.
    merge_strategy: RefCell<NullMergeStrategy>,
    stream_delay: StreamDelay,
}

impl ReplayMerger {
    pub fn new(shutdown_token: CancellationToken) -> Self {
        // FIXME configure / inject
        let stream_delay = StreamDelay::new(300, 1000);
        let merge_strategy = RefCell::new(NullMergeStrategy {});
        Self {shutdown_token, merge_strategy, stream_delay }
    }
    pub async fn lifetime(&self) {
        // TODO
    }

    pub async fn handle_connection(&self, mut c: Connection) {
        let replay = Rc::new(RefCell::new(WriterReplay::new()));
        join! {
            read_from_connection(replay.clone(), &mut c, self.shutdown_token.clone()),
            track_replay(&self.merge_strategy, &self.stream_delay, replay),
        };
    }
}
