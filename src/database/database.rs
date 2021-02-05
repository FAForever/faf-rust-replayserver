use std::time::Duration;

use crate::{config::DatabaseSettings, error::SaveError};
use sqlx::{mysql::MySqlConnectOptions, mysql::MySqlSslMode, types::time::OffsetDateTime};

// Unlike python server, we don't do arbitrary queries. We run specific queries and nothing more.
// This means we only need a real DB for this thing's unit tests, not for system tests.
pub struct Database {
    pool: sqlx::MySqlPool,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct TeamPlayerRow {
    pub login: String,
    pub team: i8,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct GameStatRow {
    pub start_time: OffsetDateTime,
    pub end_time: Option<OffsetDateTime>,
    pub game_type: String,
    pub host: String,
    pub game_name: String,
    pub game_mod: Option<String>,
    pub file_name: Option<String>,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct PlayerCount {
    pub count: i64, // In db it's signed BIGINT
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct ModVersions {
    pub file_id: u64,
    pub version: i32,
}

// TODO - SQL queries should probably be moved to some central FAF component.
// For what it's worth, I checked that inner / left joins correspond to foreign / nullable keys.
impl Database {
    pub fn new(dbc: &DatabaseSettings) -> Self {
        let options = MySqlConnectOptions::new()
            .host(dbc.host.as_str())
            .port(dbc.port)
            .username(dbc.user.as_str())
            .password(dbc.password.as_str())
            .database(dbc.name.as_str())
            .ssl_mode(MySqlSslMode::Disabled);

        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(dbc.pool_size)
            .max_lifetime(Some(Duration::from_secs(24 * 60 * 60)))
            .connect_lazy_with(options);
        Self { pool }
    }

    pub async fn close(&self) {
        self.pool.close().await
    }

    // TODO: use compile-time query checks once some integration with faf-stack is added.
    // I wonder if they interact badly with the language server.

    pub async fn get_team_players(&self, id: u64) -> Result<Vec<TeamPlayerRow>, SaveError> {
        let query = "
            SELECT
                `login`.`login` AS login,
                `game_player_stats`.`team` AS team
            FROM `game_stats`
            INNER JOIN `game_player_stats`
              ON `game_player_stats`.`gameId` = `game_stats`.`id`
            INNER JOIN `login`
              ON `login`.id = `game_player_stats`.`playerId`
            WHERE `game_stats`.`id` = ? AND `game_player_stats`.`AI` = 0
        ";
        Ok(sqlx::query_as::<_, TeamPlayerRow>(query)
            .bind(id)
            .fetch_all(&self.pool)
            .await?)
    }

    pub async fn get_game_stat_row(&self, id: u64) -> Result<GameStatRow, SaveError> {
        // TODO is table_map obsolete? Gotta ask.
        let query = "
            SELECT
                `game_stats`.`startTime` AS start_time,
                `game_stats`.`endTime` AS end_time,
                `game_stats`.`gameType` AS game_type,
                `login`.`login` AS host,
                `game_stats`.`gameName` AS game_name,
                `game_featuredMods`.`gamemod` AS game_mod,
                `table_map`.`filename` AS file_name
            FROM `game_stats`
            INNER JOIN `login`
              ON `login`.id = `game_stats`.`host`
            INNER JOIN  `game_featuredMods`
              ON `game_stats`.`gameMod` = `game_featuredMods`.`id`
            LEFT JOIN `table_map`
              ON `game_stats`.`mapId` = `table_map`.`id`
            WHERE `game_stats`.`id` = ?
        ";
        Ok(sqlx::query_as::<_, GameStatRow>(query)
            .bind(id)
            .fetch_one(&self.pool)
            .await?)
    }

    pub async fn get_player_count(&self, id: u64) -> Result<i64, SaveError> {
        let query = "
           SELECT COUNT(*) AS count FROM `game_player_stats`
           WHERE `game_player_stats`.`gameId` = ?
        ";
        Ok(sqlx::query_as::<_, PlayerCount>(query)
            .bind(id)
            .fetch_one(&self.pool)
            .await?
            .count)
    }

    pub async fn get_mod_version_list(
        &self,
        game_mod: &str,
    ) -> Result<Vec<ModVersions>, SaveError> {
        // We have to build table name dynamically, that's just how the DB is.
        // Since we know what existing tables look like, we do very restrictive validation.
        for c in game_mod.chars() {
            if !((c >= 'a' && c <= 'z')
                || (c >= 'A' && c <= 'Z')
                || (c >= '0' && c <= '9')
                || "_-".contains(c))
            {
                log::info!(
                    "Game mod '{}' has unexpected characters (outside 'a-zA-Z0-9_-'",
                    game_mod
                );
                return Ok(Vec::new());
            }
        }
        // Uses base game. Ugly check. Happens often enough for us just to skip it altogether.
        if game_mod == "ladder1v1" {
            return Ok(Vec::new());
        }
        let query = format!(
            "
            SELECT
                `updates_{game_mod}_files`.`fileId` AS file_id,
                MAX(`updates_{game_mod}_files`.`version`) AS version
            FROM `updates_{game_mod}`
            INNER JOIN `updates_{game_mod}_files` ON `fileId` = `updates_{game_mod}`.`id`
            GROUP BY `updates_{game_mod}_files`.`fileId`
        ",
            game_mod = game_mod
        );
        match sqlx::query_as::<_, ModVersions>(query.as_str())
            .fetch_all(&self.pool)
            .await
        {
            Err(e) => {
                log::warn!("Failed to query version of mod '{}': '{}'", game_mod, e);
                Ok(Vec::new())
            }
            Ok(v) => Ok(v),
        }
    }

    pub async fn update_game_stats(&self, id: u64, replay_ticks: u64) -> Result<(), SaveError> {
        let query = "
            UPDATE `game_stats` SET
                `game_stats`.`replay_ticks` = ?
            WHERE `game_stats`.`id` = ?
        ";
        sqlx::query(query)
            .bind(replay_ticks)
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod test {
    // See db_unit_test_data.sql for data used here.
    use std::collections::HashMap;

    use time::{
        macros::{date, time},
        Date, Time,
    };

    use super::*;

    fn dt(d: Date, t: Time) -> OffsetDateTime {
        d.with_time(t).assume_utc()
    }

    fn get_db() -> Database {
        let cfg = DatabaseSettings {
            host: std::env::var("DB_HOST").expect("DB_HOST is not set"),
            port: std::env::var("DB_PORT")
                .expect("DB_PORT is not set")
                .parse::<u16>()
                .expect("DB_HOST is not a number"),
            user: "root".into(),
            password: "banana".into(),
            name: "faf".into(),
            pool_size: 4,
        };
        Database::new(&cfg)
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_init() {
        let _db = get_db();
    }

    fn to_map<K: Eq + std::hash::Hash, V, F: Fn(&V) -> K>(v: Vec<V>, f: F) -> HashMap<K, V> {
        let mut m = HashMap::new();
        for p in v.into_iter() {
            m.insert(f(&p), p);
        }
        m
    }

    fn players_to_map(p: Vec<TeamPlayerRow>) -> HashMap<String, TeamPlayerRow> {
        to_map(p, |p| p.login.clone())
    }

    fn mods_to_map(m: Vec<ModVersions>) -> HashMap<u64, ModVersions> {
        to_map(m, |m| m.file_id)
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_typical_game() {
        let db = get_db();

        let stats = db.get_game_stat_row(1000).await.unwrap();
        let expected_stats = GameStatRow {
            start_time: dt(date!(2010 - 01 - 01), time!(00:00:00)),
            end_time: Some(dt(date!(2010 - 01 - 01), time!(01:00:00))),
            game_type: "0".into(),
            host: "user1".into(),
            game_name: "2v2 Game".into(),
            game_mod: Some("faf".into()),
            file_name: Some("maps/scmp_001.zip".into()),
        };
        assert_eq!(stats, expected_stats);

        assert_eq!(db.get_player_count(1000).await.unwrap(), 4);

        let players = db.get_team_players(1000).await.unwrap();
        let expected_players = vec![
            TeamPlayerRow {
                login: "user1".into(),
                team: 1,
            },
            TeamPlayerRow {
                login: "user2".into(),
                team: 1,
            },
            TeamPlayerRow {
                login: "user3".into(),
                team: 2,
            },
            TeamPlayerRow {
                login: "user4".into(),
                team: 2,
            },
        ];
        assert_eq!(players_to_map(players), players_to_map(expected_players));
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_players_with_ai() {
        // 1v1 game against AI.
        let db = get_db();

        // We count all players...
        assert_eq!(db.get_player_count(1010).await.unwrap(), 2);

        // But don't list AI players in teams.
        let players = db.get_team_players(1010).await.unwrap();
        let expected_players = vec![TeamPlayerRow {
            login: "user1".into(),
            team: 1,
        }];
        assert_eq!(players_to_map(players), players_to_map(expected_players));
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_game_on_old_map_version() {
        let db = get_db();
        let stats = db.get_game_stat_row(1030).await.unwrap();
        assert_eq!(stats.file_name.unwrap(), "maps/scmp_003.zip".to_string());
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_game_on_new_map_version() {
        let db = get_db();
        let stats = db.get_game_stat_row(1040).await.unwrap();
        assert_eq!(
            stats.file_name.unwrap(),
            "maps/scmp_003.v0003.zip".to_string()
        );
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_game_with_no_end_time() {
        let db = get_db();
        let stats = db.get_game_stat_row(1050).await.unwrap();
        assert!(stats.end_time.is_none());
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_game_with_max_nulls() {
        let db = get_db();
        let stats = db.get_game_stat_row(1060).await.unwrap();
        assert!(stats.end_time.is_none());
        assert!(stats.file_name.is_none());
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_game_with_no_players() {
        let db = get_db();
        let stats = db.get_team_players(1070).await.unwrap();
        assert!(stats.is_empty());
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_game_with_ai_only() {
        let db = get_db();
        let stats = db.get_team_players(1080).await.unwrap();
        assert!(stats.is_empty());
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_game_with_null_modname() {
        let db = get_db();
        let stats = db.get_game_stat_row(1100).await.unwrap();
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_nonexistent_game() {
        let db = get_db();
        db.get_game_stat_row(10000)
            .await
            .expect_err("Game should not exist");
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_typical_mod() {
        let db = get_db();
        let mod_data = db.get_mod_version_list("faf").await.unwrap();
        let expected_mod_data = vec![
            ModVersions {
                file_id: 41,
                version: 3659,
            },
            ModVersions {
                file_id: 42,
                version: 3659,
            },
            ModVersions {
                file_id: 43,
                version: 3656,
            },
        ];
        assert_eq!(mods_to_map(mod_data), mods_to_map(expected_mod_data));
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_ladder_mod_is_ignored() {
        let db = get_db();
        let mod_data = db.get_mod_version_list("ladder1v1").await.unwrap();
        assert!(mod_data.is_empty());
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_nonexistent_mod_is_empty() {
        let db = get_db();
        let mod_data = db.get_mod_version_list("blarlargwars").await.unwrap();
        assert!(mod_data.is_empty());
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_mod_selects_newest_file() {
        let db = get_db();
        let mod_data = db.get_mod_version_list("murderparty").await.unwrap();
        let expected_mod_data = vec![ModVersions {
            file_id: 44,
            version: 1002,
        }];
        assert_eq!(mods_to_map(mod_data), mods_to_map(expected_mod_data));
    }

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_mod_with_no_files() {
        let db = get_db();
        let mod_data = db.get_mod_version_list("labwars").await.unwrap();
        assert!(mod_data.is_empty());
    }
}
