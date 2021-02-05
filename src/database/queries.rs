use sqlx::types::time::OffsetDateTime;
use std::collections::HashMap;

use crate::error::SaveError;

use super::database::Database;

pub type GameTeams = HashMap<i8, Vec<String>>;
pub struct GameStats {
    pub featured_mod: Option<String>,
    pub game_type: String,
    pub host: String,
    pub launched_at: i64,
    pub game_end: i64,
    pub title: String,
    pub mapname: String,
    pub num_players: i64,
}
pub type ModVersions = HashMap<String, i32>;

pub struct Queries {
    db: Database,
}

impl Queries {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    pub async fn get_teams_in_game(&self, id: u64) -> Result<GameTeams, SaveError> {
        let players = self.db.get_team_players(id).await?;
        let mut res = GameTeams::new();
        for p in players.into_iter() {
            if !res.contains_key(&p.team) {
                res.insert(p.team, Vec::new());
            }
            res.get_mut(&p.team).unwrap().push(p.login);
        }
        Ok(res)
    }

    pub async fn get_game_stats(&self, id: u64) -> Result<GameStats, SaveError> {
        let stats = self.db.get_game_stat_row(id).await?;
        let player_count = self.db.get_player_count(id).await?;

        // Mapname looks like this: maps/<stuff>.zip
        // Previous two servers extracted the stuff with path.splitext(path.basename(...)).
        // Rust unix path handling is ugly and it really won't ever be anything else than this
        // (see MapService in API), so we just trim manually.
        if stats.file_name.is_none() {
            log::warn!("Map name for replay {} is missing! Saving anyway.", id);
        }
        let mapname = stats
            .file_name
            .and_then(|f| f.rsplitn(2, '.').last().map(String::from))
            .and_then(|f| f.rsplitn(2, '/').nth(0).map(String::from))
            .unwrap_or("None".into());

        Ok(GameStats {
            featured_mod: stats.game_mod,
            game_type: stats.game_type,
            host: stats.host,
            launched_at: stats.start_time.unix_timestamp(),
            game_end: stats
                .end_time
                .unwrap_or(OffsetDateTime::now_utc())
                .unix_timestamp(),
            title: stats.game_name,
            mapname,
            num_players: player_count,
        })
    }

    pub async fn get_mod_versions(&self, game_mod: &str) -> Result<ModVersions, SaveError> {
        let mut ret = HashMap::new();
        let mods = self.db.get_mod_version_list(game_mod).await?;
        for m in mods.into_iter() {
            ret.insert(m.file_id.to_string(), m.version);
        }
        Ok(ret)
    }

    pub async fn update_game_stats(&self, id: u64, replay_ticks: u64) -> Result<(), SaveError> {
        self.db.update_game_stats(id, replay_ticks).await
    }
}
