//! Runtime configuration resolved from Environment Variables

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

const ENV_CONFIG_DIR: &str = "ANYBUCKET_CONFIG_DIR";
const ENV_STATIC_DIR: &str = "ANYBUCKET_STATIC_DIR";
const ENV_PORT: &str = "ANYBUCKET_PORT";

const DEFAULT_CONFIG_DIR: &str = "/config";
const DEFAULT_STATIC_DIR: &str = "./dist";
const DEFAULT_PORT: u16 = 8080;

pub struct Config {
    /// Where connection metadata + encrypted secrets are persisted.
    pub config_dir: PathBuf,
    /// The built SPA directory served as static files with SPA fallback.
    pub static_dir: PathBuf,
    /// The socket the server binds.
    pub addr: SocketAddr,
}

impl Config {
    /// Resolve configuration from the environment, applying container-friendly defaults.
    pub fn from_env() -> Self {
        let config_dir = std::env::var(ENV_CONFIG_DIR)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_CONFIG_DIR));

        let static_dir = std::env::var(ENV_STATIC_DIR)
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(DEFAULT_STATIC_DIR));

        let port = std::env::var(ENV_PORT)
            .ok()
            .and_then(|p| p.parse::<u16>().ok())
            .unwrap_or(DEFAULT_PORT);

        Self {
            config_dir,
            static_dir,
            addr: SocketAddr::from((Ipv4Addr::UNSPECIFIED, port)),
        }
    }
}
