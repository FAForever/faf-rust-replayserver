use std::{time::Duration, collections::HashMap};

use log::error;

use crate::{config::Settings, error::SaveError};
use sqlx::types::time::OffsetDateTime;

// Unlike python server, we don't split DB into arbitrary query & our data requsts. Setting up a DB
// in order to run tests is a huge pain. We'll do it only for this struct's unit tests.
pub struct Database {
   pool: sqlx::MySqlPool,
}


#[derive(sqlx::FromRow)]
pub struct TeamPlayerRow { pub login: String, pub team: u16 }

#[derive(sqlx::FromRow)]
pub struct GameStatRow {
    pub start_time: OffsetDateTime,
    pub end_time: Option<OffsetDateTime>,
    pub game_type: String,
    pub host: String,
    pub game_name: String,
    pub game_mod: String,
    pub file_name: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct PlayerCount {pub count: u32}

impl Database {
    pub fn new(self, config: &Settings) -> Option<Self> {
        let dbc = &config.database;
        let addr = format!("mysql://{user}:{pass}!{host}:{port}/{db}",
                           user=dbc.user,
                           pass=dbc.password,
                           host=dbc.host,
                           port=dbc.port,
                           db=dbc.name);
        let pool = sqlx::mysql::MySqlPoolOptions::new()
            .max_connections(config.database.pool_size)
            .max_lifetime(Some(Duration::from_secs(24 * 60 * 60)))
            .connect_lazy(addr.as_str());
        match pool {
            Err(e) => {
                error!("Failed to connect to database: {}", e);
                None
            }
            Ok(pool) => Some(Self { pool })
        }
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
            WHERE `game_stats`.`id` = $1 AND `game_player_stats`.`AI` = 0
        ";
        Ok(sqlx::query_as::<_, TeamPlayerRow>(query).bind(id).fetch_all(&self.pool).await?)
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
            LEFT JOIN `table_map`
              ON `game_stats`.`mapId` = `table_map`.`id`
            LEFT JOIN `login`
              ON `login`.id = `game_stats`.`host`
            LEFT JOIN  `game_featuredMods`
              ON `game_stats`.`gameMod` = `game_featuredMods`.`id`
            WHERE `game_stats`.`id` = $1
        ";
        Ok(sqlx::query_as::<_, GameStatRow>(query).bind(id).fetch_one(&self.pool).await?)
    }

    pub async fn get_player_count(&self, id: u64) -> Result<u32, SaveError> {
       let query = "
           SELECT COUNT(*) AS count FROM `game_player_stats`
           WHERE `game_player_stats`.`gameId` = %s
        ";
        Ok(sqlx::query_as::<_, PlayerCount>(query).bind(id).fetch_one(&self.pool).await?.count)
    }

    pub async fn get_mod_versions(&self, game_mod: &str) {
    }

    pub async fn update_game_stats(&self, id: u64, replay_ticks: u64) {
    }
}
