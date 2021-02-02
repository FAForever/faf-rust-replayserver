use std::sync::Arc;

use crate::{database::queries::Queries, replay::streams::MReplayRef};

pub struct ReplaySaver {
    db: Queries,
}

impl ReplaySaver {
    pub fn new(db: Queries) -> Self {
        Self { db }
    }
    pub async fn save_replay(&self, replay: MReplayRef, replay_id: u64) {
        // TODO
    }
}
