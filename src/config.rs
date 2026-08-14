//! 配置加载：从 TOML 反序列化。结构对齐 Python config.toml。
use serde::{Deserialize, Serialize};

#[derive(Deserialize, Serialize, Clone)]
pub struct Config {
    pub keys: Keys,
    pub defaults: Defaults,
    pub search: Search,
    pub merge: Merge,
    pub limits: Limits,
}

#[derive(Deserialize, Serialize, Default, Clone)]
pub struct Keys {
    #[serde(default)]
    pub doubaninfo: String,
    #[serde(default)]
    pub tmdb: String,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Defaults {
    #[serde(default = "default_auto")]
    pub source: String,
    #[serde(default = "default_bbcode")]
    pub format: String,
    #[serde(default = "default_auto")]
    pub media_type: String,
    #[serde(default)]
    pub full_celebrities: bool,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Search {
    #[serde(default = "default_true")]
    pub fallback_to_tmdb: bool,
    #[serde(default = "default_10")]
    pub max_candidates: usize,
    #[serde(default)]
    pub year_strict: bool,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Merge {
    #[serde(default = "default_douban")]
    pub primary: String,
    #[serde(default = "default_true")]
    pub enrich_tmdb_rating: bool,
    #[serde(default = "default_true")]
    pub enrich_tmdb_aka: bool,
    #[serde(default = "default_true")]
    pub enrich_celebrities: bool,
}

#[derive(Deserialize, Serialize, Clone)]
pub struct Limits {
    #[serde(default = "default_2")]
    pub retry: u64,
    #[serde(default = "default_20")]
    pub timeout: u64,
    #[serde(default = "default_0_1")]
    pub min_interval: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            keys: Keys::default(),
            defaults: Defaults {
                source: default_auto(),
                format: default_bbcode(),
                media_type: default_auto(),
                full_celebrities: false,
            },
            search: Search {
                fallback_to_tmdb: true,
                max_candidates: 10,
                year_strict: false,
            },
            merge: Merge {
                primary: "douban".into(),
                enrich_tmdb_rating: true,
                enrich_tmdb_aka: true,
                enrich_celebrities: true,
            },
            limits: Limits {
                retry: 2,
                timeout: 20,
                min_interval: 0.1,
            },
        }
    }
}

fn default_auto() -> String { "auto".into() }
fn default_bbcode() -> String { "bbcode".into() }
fn default_douban() -> String { "douban".into() }
fn default_true() -> bool { true }
fn default_10() -> usize { 10 }
fn default_2() -> u64 { 2 }
fn default_20() -> u64 { 20 }
fn default_0_1() -> f64 { 0.1 }

pub fn load(path: &str) -> Result<Config, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    toml::from_str(&content).map_err(|e| e.to_string())
}
