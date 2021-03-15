use std::sync::Arc;

use crate::{
    config::Settings, database::database::Database, database::queries::Queries, metrics,
    replay::streams::MReplayRef, util::buf_traits::ReadAtExt,
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

    async fn count_and_store_ticks(&self, replay: MReplayRef, id: u64) {
        let header_len = replay.borrow().header_len();
        let maybe_ticks = scfa::parser::parse_body_ticks(&mut replay.reader_from(header_len));
        let ticks = match maybe_ticks {
            Err(e) => {
                // FIXME doesn't implement display yet
                log::info!("Failed to parse tick count for replay {}: {:?}", id, e);
                return;
            }
            Ok(t) => t,
        };
        if let Err(e) = self.db.update_game_stats(id, ticks).await {
            log::info!("Failed to update game stats for replay {}: {}", id, e);
        }
    }
    // TODO count and store ticks
    pub async fn save_replay(&self, replay: MReplayRef, id: u64) {
        if replay.borrow().get_header().is_none() {
            log::info!("Replay {} is empty, not saving.", id);
            return;
        }
        self.count_and_store_ticks(replay.clone(), id).await;
        let json_header = match ReplayJsonHeader::from_id_and_db(&self.db, id).await {
            Err(e) => {
                log::info!("Failed to fetch game {} stats from database: {}", id, e);
                return;
            }
            Ok(r) => r,
        };
        let target_file = match self.save_dir.touch_and_return_file(id).await {
            Err(e) => {
                log::warn!("Failed to create file for replay {}: {}", id, e);
                return;
            }
            Ok(f) => f,
        };
        if let Err(e) = write_replay(target_file, json_header, replay).await {
            log::warn!("Failed to write out replay {}: {}", id, e);
            return;
        }
        metrics::SAVED_REPLAYS.inc();
    }
}
