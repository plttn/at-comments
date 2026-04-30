use config::Config;
pub use config::{ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
}

#[derive(Deserialize, Clone)]
pub struct AppConfig {
    pub address: String,
    pub port: u16,
}

#[derive(Deserialize, Debug, Clone)]
pub struct PollerConfig {
    pub handle: String,
    pub emoji: String,
    pub domain: String,
}

#[derive(Deserialize, Clone)]
pub struct Settings {
    pub database: DatabaseConfig,
    pub app: AppConfig,
    pub poller: PollerConfig,
}

pub fn build_config() -> Result<Settings, ConfigError> {
    Config::builder()
        .add_source(File::with_name("Settings").required(false))
        .add_source(
            Environment::default()
                .try_parsing(true)
                .separator("_")
                .prefix("ATC"),
        )
        .build()?
        .try_deserialize()
}
