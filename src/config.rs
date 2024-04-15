use std::path::{Path, PathBuf};

use eyre::Result;
use figment::providers::{Env, Format, Toml};
use figment::value::{Dict, Map};
use figment::{Figment, Metadata, Profile, Provider};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FileConfig {
    pub search: String,
    pub replace: String,
}

impl Default for FileConfig {
    fn default() -> Self {
        FileConfig {
            search: "{current_version}".to_string(),
            replace: "{new_version}".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub current_version: semver::Version,
    pub commit: bool,
    pub tag: bool,
    pub commit_message: Option<String>,
    pub files: IndexMap<PathBuf, FileConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            current_version: semver::Version::new(0, 0, 0),
            commit: false,
            tag: false,
            commit_message: None,
            files: Default::default(),
        }
    }
}

pub static WORKDIR_CONFIG_PATH: &str = "./incrementor.toml";

impl Config {
    pub fn from<T: Provider>(provider: T) -> Result<Config> {
        Ok(Figment::from(provider).extract()?)
    }

    pub fn figment() -> Figment {
        Figment::from(Config::default())
            .merge(Toml::file(Path::new(WORKDIR_CONFIG_PATH)))
            .merge(Env::prefixed("INCREMENTOR_").split("__"))
    }
}

impl Provider for Config {
    fn metadata(&self) -> Metadata {
        Metadata::named("Incrementor config")
    }

    fn data(&self) -> Result<Map<Profile, Dict>, figment::Error> {
        figment::providers::Serialized::defaults(Config::default()).data()
    }

    fn profile(&self) -> Option<Profile> {
        None
    }
}
