use serde::Deserialize;
use std::fs;

#[derive(Debug, Clone, Deserialize)]
pub struct Config {
    #[serde(default = "default_server")]
    pub server: ServerConfig,
    #[serde(default = "default_db")]
    pub database: DbConfig,
    #[serde(default = "default_log")]
    pub log: LogConfig,
    #[serde(default)]
    pub auth: AuthConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    #[serde(default = "default_db_path")]
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LogConfig {
    #[serde(default = "default_log_level")]
    pub level: String,
    #[serde(default = "default_log_format")]
    pub format: String,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct AuthConfig {
    #[serde(default)]
    pub api_keys: Vec<String>,
}

fn default_server() -> ServerConfig {
    ServerConfig {
        host: default_host(),
        port: default_port(),
    }
}
fn default_db() -> DbConfig {
    DbConfig {
        path: default_db_path(),
    }
}
fn default_log() -> LogConfig {
    LogConfig {
        level: default_log_level(),
        format: default_log_format(),
    }
}
fn default_host() -> String { "127.0.0.1".to_string() }
fn default_port() -> u16 { 8080 }
fn default_db_path() -> String { "./stream0.db".to_string() }
fn default_log_level() -> String { "info".to_string() }
fn default_log_format() -> String { "json".to_string() }

impl Config {
    pub fn load(path: Option<&str>) -> Self {
        let mut cfg = match path {
            Some(p) => match fs::read_to_string(p) {
                Ok(data) => serde_yaml::from_str(&data).unwrap_or_else(|e| {
                    eprintln!("failed to parse config: {}", e);
                    Config::default()
                }),
                Err(_) => Config::default(),
            },
            None => Config::default(),
        };

        // Override with environment variables (only if set)
        if let Ok(v) = std::env::var("STREAM0_SERVER_HOST") {
            if !v.is_empty() { cfg.server.host = v; }
        }
        if let Ok(v) = std::env::var("STREAM0_SERVER_PORT") {
            if let Ok(port) = v.parse::<u16>() { cfg.server.port = port; }
        }
        if let Ok(v) = std::env::var("STREAM0_DB_PATH") {
            if !v.is_empty() { cfg.database.path = v; }
        }
        if let Ok(v) = std::env::var("STREAM0_LOG_LEVEL") {
            if !v.is_empty() { cfg.log.level = v; }
        }
        if let Ok(v) = std::env::var("STREAM0_LOG_FORMAT") {
            if !v.is_empty() { cfg.log.format = v; }
        }
        if let Ok(v) = std::env::var("STREAM0_API_KEY") {
            if !v.is_empty() { cfg.auth.api_keys.push(v); }
        }

        cfg
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}

impl Default for Config {
    fn default() -> Self {
        Config {
            server: default_server(),
            database: default_db(),
            log: default_log(),
            auth: AuthConfig::default(),
        }
    }
}
