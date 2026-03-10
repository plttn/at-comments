pub use config::{Config, ConfigError, Environment, File};
use serde::Deserialize;

#[derive(Deserialize)]
#[allow(unused)]
struct DatabaseConfig {
    url: String,
}
#[derive(Deserialize)]
#[allow(unused)]
struct AppConfig {
    address: String,
    port: u16,
}
#[derive(Deserialize)]
#[allow(unused)]
struct PollerConfig {
    handle: String,
    emoji: String,
    domain: String,
}
#[derive(Deserialize)]
#[allow(unused)]
pub struct Settings {
    database: DatabaseConfig,
    app: AppConfig,
    poller: PollerConfig,
}

pub fn build_config() -> Result<Config, ConfigError> {
    Config::builder()
        .add_source(File::with_name("Settings").required(false))
        .add_source(
            Environment::default()
                .try_parsing(true)
                .separator("_")
                .prefix("ATC"),
        )
        .build()
}
