use std::{env, sync::Arc};

use config::{Config, ConfigError, File};
use serde::Deserialize;

// TODO - validate values.

#[derive(Debug, Deserialize, Clone)]
pub struct ServerSettings {
    pub port: u16,
    pub prometheus_port: u16,
    pub worker_threads: u32,
    pub connection_accept_timeout_s: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseSettings {
    pub pool_size: u32,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct StorageSettings {
    pub vault_path: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ReplaySettings {
    pub forced_timeout_s: u64,
    pub time_with_zero_writers_to_end_replay_s: u64,
    pub delay_s: u64,
    pub update_interval_ms: u64,
    pub merge_quorum_size: usize,
    pub stream_comparison_distance_b: usize,
}

pub type Settings = Arc<InnerSettings>;

#[derive(Debug, Deserialize)]
pub struct InnerSettings {
    pub server: ServerSettings,
    pub database: DatabaseSettings,
    pub storage: StorageSettings,
    pub replay: ReplaySettings,
}

impl InnerSettings {
    pub fn from_env() -> Result<Arc<Self>, ConfigError> {
        let config_file = env::var("RS_CONFIG_FILE").map_err(|_| {
            ConfigError::Message(
                "RS_CONFIG_FILE env var not set, place the path to the config file there.".into(),
            )
        })?;
        let db_password = env::var("RS_DB_PASSWORD")
            .map_err(|_| ConfigError::NotFound("Database password was not provided".into()))?;
        let mut c = Config::new();
        c.set("database.password", db_password)?;
        c.merge(File::with_name(&config_file[..]))?;

        Ok(Arc::new(c.try_into()?))
    }
}

#[cfg(test)]
pub mod test {
    use super::*;

    pub fn default_config() -> InnerSettings {
        InnerSettings {
            server: ServerSettings {
                port: 15000,
                prometheus_port: 8001,
                worker_threads: 8,
                connection_accept_timeout_s: 7200,
            },
            database: DatabaseSettings {
                pool_size: 8,
                host: "localhost".into(),
                port: 3306,
                user: "root".into(),
                password: "banana".into(),
                name: "faf".into(),
            },
            storage: StorageSettings {
                vault_path: "/tmp/foo".into(),
            },
            replay: ReplaySettings {
                forced_timeout_s: 3600 * 6,
                time_with_zero_writers_to_end_replay_s: 10,
                delay_s: 60 * 5,
                update_interval_ms: 1000,
                merge_quorum_size: 2,
                stream_comparison_distance_b: 4096,
            },
        }
    }
}
