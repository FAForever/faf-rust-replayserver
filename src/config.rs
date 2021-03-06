use std::{
    env::{self, VarError},
    sync::Arc,
};

use config::{Config, ConfigError, File};
use serde::Deserialize;

// TODO - implement some validation.

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ServerSettings {
    pub port: u16,
    pub prometheus_port: u16,
    pub worker_threads: u32,
    pub connection_accept_timeout_s: u64,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct DatabaseSettings {
    pub pool_size: u32,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub password: String,
    pub name: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct StorageSettings {
    pub vault_path: String,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ReplaySettings {
    pub forced_timeout_s: u64,
    pub time_with_zero_writers_to_end_replay_s: u64,
    pub delay_s: u64,
    pub update_interval_ms: u64,
    pub merge_quorum_size: usize,
    pub stream_comparison_distance_b: usize,
}

pub type Settings = Arc<InnerSettings>;

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct InnerSettings {
    pub server: ServerSettings,
    pub database: DatabaseSettings,
    pub storage: StorageSettings,
    pub replay: ReplaySettings,
}

impl InnerSettings {
    pub fn from_env() -> Result<Arc<Self>, ConfigError> {
        let conf_var = env::var("RS_CONFIG_FILE");
        let db_pass_var = env::var("RS_DB_PASSWORD");
        Self::do_from_env(conf_var, db_pass_var).map(|x| Arc::new(x))
    }
    fn do_from_env(
        conf_var: Result<String, VarError>,
        db_pass_var: Result<String, VarError>,
    ) -> Result<Self, ConfigError> {
        let config_file = conf_var.map_err(|_| {
            ConfigError::Message(
                "RS_CONFIG_FILE env var not set, place the path to the config file there.".into(),
            )
        })?;
        let db_password = db_pass_var
            .map_err(|_| ConfigError::NotFound("Database password was not provided".into()))?;
        let mut c = Config::new();
        c.set("database.password", db_password)?;
        c.merge(File::with_name(&config_file[..]))?;
        c.try_into()
    }
}

#[cfg(test)]
pub mod test {
    use crate::util::test::get_file_path;

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

    #[test]
    fn test_example_config_load() {
        let conf_file = get_file_path("example_config.yml");
        let password = String::from("banana"); // File does not have a password entry
        let conf = InnerSettings::do_from_env(Ok(conf_file), Ok(password)).unwrap();
        assert_eq!(conf, default_config());
    }

    #[test]
    fn test_config_needs_password() {
        let conf_file = get_file_path("example_config.yml");
        InnerSettings::do_from_env(Ok(conf_file), Err(VarError::NotPresent))
            .expect_err("Config init should've failed");
    }

    #[test]
    fn test_config_needs_file() {
        let password = String::from("banana"); // File does not have a password entry
        InnerSettings::do_from_env(Err(VarError::NotPresent), Ok(password))
            .expect_err("Config init should've failed");
    }
}
