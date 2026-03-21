use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

// --- Server Config ---

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_db_path")]
    pub db_path: String,
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_db_path() -> String {
    "./bh.db".to_string()
}
fn default_log_level() -> String {
    "info".to_string()
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: default_host(),
            port: default_port(),
            db_path: default_db_path(),
            log_level: default_log_level(),
        }
    }
}

impl ServerConfig {
    pub fn load(path: Option<&str>) -> Self {
        let mut cfg = match path {
            Some(p) => match fs::read_to_string(p) {
                Ok(data) => toml::from_str(&data).unwrap_or_else(|e| {
                    eprintln!("failed to parse config: {}", e);
                    ServerConfig::default()
                }),
                Err(_) => ServerConfig::default(),
            },
            None => ServerConfig::default(),
        };

        if let Ok(v) = std::env::var("BH_HOST") {
            if !v.is_empty() {
                cfg.host = v;
            }
        }
        if let Ok(v) = std::env::var("BH_PORT") {
            if let Ok(port) = v.parse::<u16>() {
                cfg.port = port;
            }
        }
        if let Ok(v) = std::env::var("BH_DB_PATH") {
            if !v.is_empty() {
                cfg.db_path = v;
            }
        }
        if let Ok(v) = std::env::var("BH_LOG_LEVEL") {
            if !v.is_empty() {
                cfg.log_level = v;
            }
        }

        cfg
    }

    pub fn address(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

// --- CLI Config ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CliConfig {
    #[serde(default = "default_server_url")]
    pub server_url: String,
    #[serde(default)]
    pub lead_id: Option<String>,
    #[serde(default)]
    pub api_key: Option<String>,
}

fn default_server_url() -> String {
    "http://localhost:8080".to_string()
}

impl Default for CliConfig {
    fn default() -> Self {
        CliConfig {
            server_url: default_server_url(),
            lead_id: None,
            api_key: None,
        }
    }
}

impl CliConfig {
    fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".bh")
    }

    fn config_path() -> PathBuf {
        Self::config_dir().join("config.toml")
    }

    fn pending_path() -> PathBuf {
        Self::config_dir().join("pending.json")
    }

    pub fn load() -> Self {
        let path = Self::config_path();
        match fs::read_to_string(&path) {
            Ok(data) => toml::from_str(&data).unwrap_or_default(),
            Err(_) => CliConfig::default(),
        }
    }

    pub fn save(&self) -> anyhow::Result<()> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir)?;
        let data = toml::to_string_pretty(self)?;
        fs::write(Self::config_path(), data)?;
        Ok(())
    }

    /// Get or create a stable lead ID.
    pub fn lead_id(&mut self) -> String {
        if let Some(ref id) = self.lead_id {
            return id.clone();
        }
        let id = format!("lead-{}", &uuid::Uuid::new_v4().to_string()[..8]);
        self.lead_id = Some(id.clone());
        let _ = self.save();
        id
    }

    /// Get the server URL, with env var override.
    pub fn server_url(&self) -> String {
        if let Ok(v) = std::env::var("BH_SERVER_URL") {
            if !v.is_empty() {
                return v;
            }
        }
        self.server_url.clone()
    }

    pub fn load_pending() -> PendingState {
        let path = Self::pending_path();
        match fs::read_to_string(&path) {
            Ok(data) => serde_json::from_str(&data).unwrap_or_default(),
            Err(_) => PendingState::default(),
        }
    }

    pub fn save_pending(state: &PendingState) -> anyhow::Result<()> {
        let dir = Self::config_dir();
        fs::create_dir_all(&dir)?;
        let data = serde_json::to_string_pretty(state)?;
        fs::write(Self::pending_path(), data)?;
        Ok(())
    }

    // --- Skill Installation ---

    fn skill_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join("skills")
            .join("bh")
    }

    fn skill_path() -> PathBuf {
        Self::skill_dir().join("SKILL.md")
    }

    pub fn install_skill(server_url: &str) -> anyhow::Result<()> {
        fs::create_dir_all(Self::skill_dir())?;

        let skill_content = format!(
r#"---
name: bh
description: |
  Delegate tasks to specialized AI workers via Boxhouse.
  Use when the user asks to review code, check security, run tests,
  or any task that matches a registered worker's expertise.
allowed-tools:
  - Bash
---

# Boxhouse (`bh`) — Agent Delegation

You have access to a team of specialized AI workers managed by Boxhouse.
The server is at: {server_url}

## When to delegate

When the user's request matches a worker's expertise, delegate instead of doing it yourself.
Examples: "review this PR", "check for security issues", "write tests", "update the docs".

Run `bh worker ls` to see available workers and their specializations.

## Commands

```bash
# Delegate a task (non-blocking, returns immediately)
bh delegate <worker> "<detailed task prompt>"

# Wait for results
bh wait

# Quick one-off task (no named worker needed)
bh worker temp "<task>"

# Reply to a worker's question
bh reply <thread-id> "<answer>"

# Manage workers
bh worker ls
bh worker add <name> --instructions "..."
bh worker remove <name>
```

## How to write delegation prompts

Do NOT just forward the user's words. Compose a complete, actionable prompt:

1. **Gather context** — which repo, branch, files, what changed
2. **Be specific** — include file paths, line numbers, relevant details
3. **State the goal** — what the worker should produce

Bad: "review this PR"
Good: "Review the changes on branch feature-timeout in this repo. The PR adds timeout handling to src/handler.rs. Focus on correctness and edge cases. Cite line numbers."

## Concurrent tasks

You can delegate to multiple workers in parallel:

```bash
bh delegate reviewer "Review the changes on branch feature-timeout..."
bh delegate security "Check src/handler.rs for security vulnerabilities..."
bh wait
```

`bh wait` blocks and streams results as workers complete.

## Worker questions

If a worker asks a question during `bh wait`, you'll see it. Use `bh reply <thread-id> "<answer>"` to respond. The worker will continue after receiving your answer.
"#,
            server_url = server_url
        );

        fs::write(Self::skill_path(), skill_content)?;
        Ok(())
    }

    pub fn uninstall_skill() -> anyhow::Result<()> {
        let dir = Self::skill_dir();
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        // Also clean up old format (bh.md plain file)
        let old_path = dir.with_extension("md");
        if old_path.exists() {
            let _ = fs::remove_file(&old_path);
        }
        Ok(())
    }

    pub fn clear(self) -> anyhow::Result<()> {
        let config_path = Self::config_path();
        if config_path.exists() {
            fs::remove_file(&config_path)?;
        }
        let pending_path = Self::pending_path();
        if pending_path.exists() {
            fs::remove_file(&pending_path)?;
        }
        Ok(())
    }
}

// --- Pending State ---

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PendingState {
    #[serde(default)]
    pub threads: std::collections::HashMap<String, PendingThread>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingThread {
    pub worker: String,
    pub task: String,
    pub created_at: String,
    /// If true, the worker is temporary and should be removed when the task completes.
    #[serde(default)]
    pub temp: bool,
}
