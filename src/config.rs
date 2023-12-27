use std::path::PathBuf;

use once_cell::sync::Lazy;
use serde::Deserialize;

pub static CONFIG: Lazy<Config> =
    Lazy::new(|| envy::from_env().expect("Failed to load Environment variable"));

#[derive(Deserialize, Debug)]
pub struct Config {
    pub command_prefix: Option<String>,
    pub voicevox_host: String,
    pub discord_token: String,
    pub additional_headers: Option<String>,
    pub persistent_path: PathBuf,
    pub emoji_path: PathBuf
}
