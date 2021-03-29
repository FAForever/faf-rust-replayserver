use std::sync::Arc;

use crate::{
    database::database::Database, database::queries::Queries, metrics, replay::streams::MReplayRef,
    util::buf_traits::ReadAtExt,
};

use super::{writer::write_replay, ReplayJsonHeader, SavedReplayDirectory};
use faf_replay_parser::scfa;

pub type ReplaySaver = Arc<InnerReplaySaver>;

#[cfg_attr(test, faux::create)]
pub struct InnerReplaySaver {
    db: Queries,
    save_dir: SavedReplayDirectory,
}

impl InnerReplaySaver {
    pub fn new(db: Database, save_dir: SavedReplayDirectory) -> Arc<Self> {
        Arc::new(Self::new_inner(db, save_dir))
    }
}

#[cfg_attr(test, faux::methods)]
impl InnerReplaySaver {
    fn new_inner(db: Database, save_dir: SavedReplayDirectory) -> Self {
        Self {
            db: Queries::new(db),
            save_dir,
        }
    }

    async fn count_ticks(&self, replay: MReplayRef, id: u64) -> Option<u32> {
        if replay.borrow().get_header().is_none() {
            return None;
        }
        let header_len = replay.borrow().header_len();
        let ticks = scfa::parser::parse_body_ticks(&mut replay.reader_from(header_len));
        if let Err(e) = &ticks {
            log::info!("Failed to parse tick count for replay {}: {}", id, e);
        }
        ticks.ok()
    }

    async fn save_replay_to_disk(&self, replay: MReplayRef, id: u64) -> bool {
        if replay.borrow().get_header().is_none() {
            log::info!("Replay {} is empty, not saving.", id);
            return false;
        }
        let json_header = match ReplayJsonHeader::from_id_and_db(&self.db, id).await {
            Err(e) => {
                log::info!("Failed to fetch game {} stats from database: {}", id, e);
                return false;
            }
            Ok(r) => r,
        };
        let target_file = match self.save_dir.touch_and_return_file(id).await {
            Err(e) => {
                log::warn!("Failed to create file for replay {}: {}", id, e);
                return false;
            }
            Ok(f) => f,
        };
        if let Err(e) = write_replay(target_file, json_header, replay).await {
            log::warn!("Failed to write out replay {}: {}", id, e);
            return false;
        }
        metrics::SAVED_REPLAYS.inc();
        return true;
    }
    // TODO count and store ticks
    pub async fn save_replay(&self, replay: MReplayRef, id: u64) {
        let replay_saved = self.save_replay_to_disk(replay.clone(), id).await;
        let ticks = self.count_ticks(replay, id).await;
        if let Err(e) = self.db.update_game_stats(id, ticks, replay_saved).await {
            log::info!("Failed to update game stats for replay {}: {}", id, e);
        }
    }
}
