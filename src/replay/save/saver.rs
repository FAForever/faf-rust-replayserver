use std::{io::Read, sync::Arc};

use crate::{
    config::Settings, database::database::Database, database::queries::Queries, metrics, replay::streams::MReplayRef,
    util::buf_traits::ReadAtExt,
};

use super::{writer::write_replay_file, ReplayJsonHeader, SavedReplayDirectory};
use faf_replay_parser::scfa;

pub type ReplaySaver = Arc<InnerReplaySaver>;

#[cfg_attr(test, faux::create)]
pub struct InnerReplaySaver {
    db: Queries,
    save_dir: SavedReplayDirectory,
    compression_level: u32,
}

impl InnerReplaySaver {
    pub fn new(db: Database, save_dir: SavedReplayDirectory, config: &Settings) -> Arc<Self> {
        Arc::new(Self::new_inner(db, save_dir, config))
    }
}

#[cfg_attr(test, faux::methods)]
impl InnerReplaySaver {
    fn new_inner(db: Database, save_dir: SavedReplayDirectory, config: &Settings) -> Self {
        let compression_level = config.storage.compression_level;
        Self {
            db: Queries::new(db),
            save_dir,
            compression_level,
        }
    }

    fn get_ticks(&self, mut data: impl Read, id: u64) -> Option<u32> {
        let ticks = scfa::parser::parse_body_ticks(&mut data);
        if let Err(e) = &ticks {
            log::info!("Failed to parse tick count for replay {}: {}", id, e);
        }
        ticks.ok()
    }

    fn count_ticks(&self, replay: MReplayRef, id: u64) -> Option<u32> {
        replay.borrow().get_header()?;
        let header_len = replay.borrow().header_len();
        self.get_ticks(&mut replay.reader_from(header_len), id)
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
        if let Err(e) = write_replay_file(target_file, json_header, replay, self.compression_level).await {
            log::warn!("Failed to write out replay {}: {}", id, e);
            return false;
        }
        metrics::SAVED_REPLAYS.inc();
        return true;
    }

    pub async fn save_replay(&self, replay: MReplayRef, id: u64) {
        let replay_saved = self.save_replay_to_disk(replay.clone(), id).await;
        let ticks = self.count_ticks(replay, id);
        if let Err(e) = self.db.update_game_stats(id, ticks, replay_saved).await {
            log::info!("Failed to update game stats for replay {}: {}", id, e);
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::config::test::default_config;
    use crate::util::test::get_file;

    #[test]
    fn saver_can_read_example_replay_ticks() {
        let config = Arc::new(default_config());
        let example_replay = get_file("example_body");
        let mock_db = Database::faux();
        let mock_dir = SavedReplayDirectory::faux();
        let saver = InnerReplaySaver::new_inner(mock_db, mock_dir, &config);
        let ticks = saver.get_ticks(&example_replay[..], 1);
        assert!(ticks.is_some());
    }
}
