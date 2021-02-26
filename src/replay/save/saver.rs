use std::sync::Arc;

use crate::{
    config::Settings, database::database::Database, database::queries::Queries,
    replay::streams::MReplayRef,
};

use super::{writer::write_replay, ReplayJsonHeader, SavedReplayDirectory};

pub type ReplaySaver = Arc<InnerReplaySaver>;

#[cfg_attr(test, faux::create)]
pub struct InnerReplaySaver {
    db: Queries,
    save_dir: SavedReplayDirectory,
}

impl InnerReplaySaver {
    pub fn new(db: Database, config: Settings) -> Arc<Self> {
        Arc::new(Self::new_inner(db, config))
    }
}

#[cfg_attr(test, faux::methods)]
impl InnerReplaySaver {
    fn new_inner(db: Database, config: Settings) -> Self {
        Self {
            db: Queries::new(db),
            save_dir: SavedReplayDirectory::new(config.storage.vault_path.as_ref()),
        }
    }
    // TODO count and store ticks
    pub async fn save_replay(&self, replay: MReplayRef, replay_id: u64) {
        if replay.borrow().get_header().is_none() {
            log::info!("Replay {} is empty, not saving.", replay_id);
            return;
        }
        let json_header = match ReplayJsonHeader::from_id_and_db(&self.db, replay_id).await {
            Err(e) => {
                log::warn!(
                    "Failed to fetch game {} stats from the database: {}",
                    replay_id,
                    e
                );
                return;
            }
            Ok(r) => r,
        };
        let target_file = match self.save_dir.touch_and_return_file(replay_id).await {
            Err(e) => {
                log::warn!("Failed to create file for replay {}: {}", replay_id, e);
                return;
            }
            Ok(f) => f,
        };
        if let Err(e) = write_replay(target_file, json_header, replay).await {
            log::warn!("Failed to write out replay {}: {}", replay_id, e);
        }
    }
}
