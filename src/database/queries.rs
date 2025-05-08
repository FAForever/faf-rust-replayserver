use sqlx::types::time::OffsetDateTime;
use std::collections::BTreeMap;

use crate::error::SaveError;

use super::database::Database;

pub type GameTeams = BTreeMap<i8, Vec<String>>;
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
pub type ModVersions = BTreeMap<String, i32>;

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
            let plist = res.entry(p.team).or_insert_with(Vec::new);
            plist.push(p.login);
        }
        Ok(res)
    }

    fn unmangle_map_name(name: Option<String>) -> String {
        // Mapname looks like this: maps/<stuff>.zip
        // Previous two servers extracted the stuff with path.splitext(path.basename(...)).
        // Rust unix path handling is ugly and it really won't ever be anything else than this
        // (see MapService in API), so we just trim manually.
        name.and_then(|f| f.rsplitn(2, '.').last().map(String::from))
            .and_then(|f| f.rsplitn(2, '/').next().map(String::from))
            .unwrap_or_else(|| "None".into())
    }

    pub async fn get_game_stats(&self, id: u64) -> Result<GameStats, SaveError> {
        let stats = self.db.get_game_stat_row(id).await?;
        let player_count = self.db.get_player_count(id).await?;

        if stats.file_name.is_none() {
            log::info!("Map name for replay {} is missing! Saving anyway.", id);
        }
        let mapname = Self::unmangle_map_name(stats.file_name);

        Ok(GameStats {
            featured_mod: stats.game_mod,
            game_type: stats.game_type,
            host: stats.host,
            launched_at: stats.start_time.unix_timestamp(),
            game_end: stats.end_time.unwrap_or_else(OffsetDateTime::now_utc).unix_timestamp(),
            title: stats.game_name,
            mapname,
            num_players: player_count,
        })
    }

    pub async fn get_mod_versions(&self, game_mod: &str) -> Result<ModVersions, SaveError> {
        let mut ret = BTreeMap::new();
        let mods = self.db.get_mod_version_list(game_mod).await?;
        for m in mods.into_iter() {
            ret.insert(m.file_id.to_string(), m.version);
        }
        Ok(ret)
    }

    pub async fn update_game_stats(
        &self,
        id: u64,
        replay_ticks: Option<u32>,
        replay_available: bool,
    ) -> Result<(), SaveError> {
        self.db.update_game_stats(id, replay_ticks, replay_available).await
    }
}

#[cfg(test)]
mod test {
    use time::macros::{date, time};

    use crate::database::database::test::mock_database;
    use crate::database::database::GameStatRow;
    use crate::util::test::dt;

    use super::*;

    #[tokio::test]
    async fn test_teams_in_game() {
        let q = Queries::new(mock_database());
        let teams = q.get_teams_in_game(1).await.unwrap();

        assert!(teams.len() == 2);
        let mut t1 = teams[&1].clone();
        t1.sort();
        assert_eq!(t1, vec![String::from("user1"), "user2".into()]);
        let mut t2 = teams[&2].clone();
        t2.sort();
        assert_eq!(t2, vec![String::from("user3"), "user4".into()]);
    }

    #[tokio::test]
    async fn test_usual_game_stats() {
        let q = Queries::new(mock_database());
        let stats = q.get_game_stats(1).await.unwrap();

        // Check just a few interesting bits.
        assert_eq!(stats.featured_mod, Some("faf".into()));
        assert_eq!(stats.launched_at, 1262304000); // 2010-01-01 00:00:00
        assert_eq!(stats.game_end, 1262307600); // 2010-01-01 01:00:00
        assert_eq!(stats.mapname, "scmp_001");
        assert_eq!(stats.num_players, 4);
    }

    #[tokio::test]
    async fn test_game_stats_none_fields() {
        let mut mdb = Database::faux();
        let null_stats = || GameStatRow {
            start_time: dt(date!(2010 - 01 - 01), time!(00:00:00)),
            end_time: None,
            game_type: "DEMORALIZATION".into(),
            host: "user1".into(),
            game_name: "2v2 Game".into(),
            game_mod: Some("faf".into()),
            file_name: None,
        };
        faux::when!(mdb.get_game_stat_row).then(move |_| Ok(null_stats()));
        faux::when!(mdb.get_player_count).then(|_| Ok(4));

        let q = Queries::new(mdb);
        let stats = q.get_game_stats(1).await.unwrap();

        assert_eq!(stats.mapname, "None");
        // We only check if this is recent enough, only other way to check is use the same API we
        // generated this with
        assert!(stats.game_end > 1577836800); // 2021-01-01 00:00:00
    }

    #[tokio::test]
    async fn test_game_mod_versions() {
        let q = Queries::new(mock_database());
        let mods = q.get_mod_versions("foo").await.unwrap();
        assert_eq!(mods.len(), 3);
        assert_eq!(mods["50"], 3000);
        assert_eq!(mods["60"], 3001);
        assert_eq!(mods["70"], 3002);
    }
}
