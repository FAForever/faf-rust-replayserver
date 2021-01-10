use std::{cell::RefCell, rc::Rc};

use tokio::join;
use tokio_util::sync::CancellationToken;

use crate::server::connection::Connection;

use super::{writer_replay::{WriterReplay, read_from_connection}, merge_strategy::NullMergeStrategy, merge_strategy::track_replay};

pub struct ReplayMerger {
    shutdown_token: CancellationToken,
    // Will be a boxed trait if ever needed.
    merge_strategy: RefCell<NullMergeStrategy>,
}

impl ReplayMerger {
    pub fn new(shutdown_token: CancellationToken) -> Self {
        Self {shutdown_token, merge_strategy: RefCell::new(NullMergeStrategy {}) }
    }
    pub async fn lifetime(&self) {
        // TODO
    }
    pub async fn handle_connection(&self, mut c: Connection) {
        let replay = Rc::new(RefCell::new(WriterReplay::new()));
        join! {
            read_from_connection(replay.clone(), &mut c, self.shutdown_token.clone()),
            track_replay(&self.merge_strategy, replay),
        };
    }
}
