use std::{
    env::{self, VarError},
    sync::Arc,
    time::Duration,
};

use config::{Config, ConfigError, File};
use serde::Deserialize;

// TODO - implement some validation.

mod float_to_duration {
    use super::*;
    use serde::{de::Unexpected, Deserialize, Deserializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        let duration = f64::deserialize(deserializer)?;
        if duration <= 0f64 {
            return Err(serde::de::Error::invalid_value(
                Unexpected::Float(duration),
                &"Positive number",
            ));
        }
        Ok(Duration::from_secs_f64(duration))
    }
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ServerSettings {
    pub port: Option<u16>,
    pub websocket_port: Option<u16>,
    pub prometheus_port: u16,
    pub worker_threads: u32,
    #[serde(with = "float_to_duration")]
    pub connection_accept_timeout_s: Duration,
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
    pub compression_level: u32,
}

#[derive(Debug, Deserialize, Clone, PartialEq, Eq)]
pub struct ReplaySettings {
    #[serde(with = "float_to_duration")]
    pub forced_timeout_s: Duration,
    #[serde(with = "float_to_duration")]
    pub time_with_zero_writers_to_end_replay_s: Duration,
    #[serde(with = "float_to_duration")]
    pub delay_s: Duration,
    #[serde(with = "float_to_duration")]
    pub update_interval_s: Duration,
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
        Self::do_from_env(conf_var, db_pass_var).map(Arc::new)
    }

    fn do_from_env(
        conf_var: Result<String, VarError>,
        db_pass_var: Result<String, VarError>,
    ) -> Result<Self, ConfigError> {
        let config_file = conf_var.map_err(|_| {
            ConfigError::Message("RS_CONFIG_FILE env var not set, place the path to the config file there.".into())
        })?;
        let db_password =
            db_pass_var.map_err(|_| ConfigError::NotFound("Database password was not provided".into()))?;
        let c = Config::builder()
            .set_override("database.password", db_password)?
            .add_source(File::with_name(&config_file[..]))
            .build()?;
        let ret: Self = c.try_deserialize()?;
        if ret.server.port.is_none() && ret.server.websocket_port.is_none() {
            return Err(ConfigError::Message("At least one of port, websocket_port must be set in server configuration.".into()));
        }
        Ok(ret)
    }
}

#[cfg(test)]
pub mod test {
    use crate::util::test::get_file_path;

    use super::*;

    pub fn default_config() -> InnerSettings {
        InnerSettings {
            server: ServerSettings {
                port: Some(15000),
                websocket_port: None,
                prometheus_port: 8001,
                worker_threads: 8,
                connection_accept_timeout_s: Duration::from_secs(7200),
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
                compression_level: 10,
            },
            replay: ReplaySettings {
                forced_timeout_s: Duration::from_secs(3600 * 6),
                time_with_zero_writers_to_end_replay_s: Duration::from_secs(10),
                delay_s: Duration::from_secs(60 * 5),
                update_interval_s: Duration::from_secs(1),
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
        InnerSettings::do_from_env(Ok(conf_file), Err(VarError::NotPresent)).expect_err("Config init should've failed");
    }

    #[test]
    fn test_config_needs_file() {
        let password = String::from("banana"); // File does not have a password entry
        InnerSettings::do_from_env(Err(VarError::NotPresent), Ok(password)).expect_err("Config init should've failed");
    }
}
