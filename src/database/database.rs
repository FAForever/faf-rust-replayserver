use std::time::Duration;

use log::error;

use crate::{config::DatabaseSettings, error::SaveError};
use sqlx::{types::time::OffsetDateTime, mysql::MySqlSslMode, mysql::MySqlConnectOptions};

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
    pub game_mod: String,
    pub file_name: Option<String>,
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct PlayerCount {
    pub count: i64,     // In db it's signed BIGINT
}

#[derive(sqlx::FromRow, Debug, PartialEq, Eq)]
pub struct ModVersions {
    pub file_id: u64,
    pub version: u64,
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
            .ssl_mode(MySqlSslMode::Disabled); // FIXME do we use SSL? Should this be configurable?

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

use time::{macros::{date, time}, Date, Time};

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

    #[cfg_attr(not(feature = "local_db_tests"), ignore)]
    #[tokio::test]
    async fn test_db_typical_game() {
        let db = get_db();

        let stats = db.get_game_stat_row(1000).await.unwrap();
        let expected_stats = GameStatRow {
            start_time: dt(date!(2010-01-01), time!(00:00:00)),
            end_time: Some(dt(date!(2010-01-01), time!(01:00:00))),
            game_type: "0".into(),
            host: "user1".into(),
            game_name: "2v2 Game".into(),
            game_mod: "faf".into(),
            file_name: Some("maps/scmp_001.zip".into()),
        };
        assert_eq!(stats, expected_stats);

        assert_eq!(db.get_player_count(1000).await.unwrap(), 4);

        let players = db.get_team_players(1000).await.unwrap();
        let expected_players = vec!(
            TeamPlayerRow { login: "user1".into(), team: 1 },
            TeamPlayerRow { login: "user2".into(), team: 1 },
            TeamPlayerRow { login: "user3".into(), team: 2 },
            TeamPlayerRow { login: "user4".into(), team: 2 },
        );
        let to_map = |v: Vec<TeamPlayerRow>| {
            let mut player_map = HashMap::new();
            for p in v.into_iter() {
                player_map.insert(p.login.clone(), p);
            }
            player_map
        };
        assert_eq!(to_map(players), to_map(expected_players));
    }
}
