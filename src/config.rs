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
    "./b0.db".to_string()
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

        if let Ok(v) = std::env::var("B0_HOST") {
            if !v.is_empty() {
                cfg.host = v;
            }
        }
        if let Ok(v) = std::env::var("B0_PORT") {
            if let Ok(port) = v.parse::<u16>() {
                cfg.port = port;
            }
        }
        if let Ok(v) = std::env::var("B0_DB_PATH") {
            if !v.is_empty() {
                cfg.db_path = v;
            }
        }
        if let Ok(v) = std::env::var("B0_LOG_LEVEL") {
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
    #[serde(default)]
    pub default_group: Option<String>,
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
            default_group: None,
        }
    }
}

impl CliConfig {
    fn config_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".b0")
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
        if let Ok(v) = std::env::var("B0_SERVER_URL") {
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

    const SKILL_MARKER_START: &str = "<!-- box0-skill-start -->";
    const SKILL_MARKER_END: &str = "<!-- box0-skill-end -->";

    /// Generate the core skill content (agent-agnostic).
    pub fn skill_content(server_url: &str) -> String {
        format!(
r#"# Box0 (`b0`) Agent Delegation

You have access to a team of specialized AI workers managed by Box0.
The server is at: {server_url}

## When to use

When the user's request could benefit from specialized workers or parallel execution, delegate.

Run `b0 worker ls` to see available workers and their descriptions. Match workers to the task based on their descriptions. You do not need the user to name specific workers.

## Commands

```bash
b0 worker ls                                          # list available workers
b0 delegate <worker> "<detailed task prompt>"         # send task (non-blocking)
b0 delegate --thread <id> <worker> "<follow-up>"      # continue conversation
b0 wait                                                # collect all pending results
b0 reply <thread-id> "<answer>"                        # answer a worker's question
b0 status                                              # check pending tasks
b0 worker temp "<task>"                                # one-off task, no named worker
```

## How to write delegation prompts

This is critical. Do NOT forward the user's words. Compose a complete, actionable prompt.

Bad:
```
b0 delegate reviewer "review this PR"
```

Good:
```
b0 delegate reviewer "Review the changes on branch feature-timeout in this repo.
The PR adds timeout handling to src/handler.rs.
Focus on correctness, edge cases, and error handling.
Cite line numbers for any issues found."
```

Steps:
1. **Gather context first** — read relevant files, run `git diff`, check the branch
2. **Include specifics** — file paths, line numbers, branch names, what changed and why
3. **State the deliverable** — what the worker should produce (a list of issues, a summary, a fix)

For large content (diffs, file contents), pipe via stdin:
```
git diff main..HEAD | b0 delegate reviewer "Review the following diff. Focus on correctness."
```

## Concurrent tasks

Delegate to multiple workers, then collect all results:

```bash
b0 delegate reviewer "Review the changes on branch feature-timeout..."
b0 delegate security "Check src/handler.rs for OWASP top 10 vulnerabilities..."
b0 delegate doc-writer "Update README to reflect the new timeout config option..."
b0 wait
```

All three run in parallel. `b0 wait` blocks until all complete.

## Handling worker questions

During `b0 wait`, a worker may ask a question:

```
reviewer asks (thread thread-abc): "Is the timeout change on line 42 intentional?"
  → Use: b0 reply thread-abc "<your answer>"
```

Answer with `b0 reply`, then run `b0 wait` again to continue collecting results.

## Proactive status checks

Before responding to a new user message, run `b0 status` to check if any previously delegated tasks have completed. Report results to the user if any are ready.

## Error handling

If a worker fails, `b0 wait` reports it. Decide whether to:
- Retry with a clearer prompt
- Try a different worker
- Handle the task yourself
- Report the failure to the user

## Multi-turn conversations

To continue a conversation with a worker, pass the thread ID from the first round:

```bash
b0 delegate --thread <thread-id> <worker> "<follow-up>"
b0 wait
```

The worker remembers all previous turns.
"#,
            server_url = server_url
        )
    }

    /// Install skill for Claude Code: ~/.claude/skills/bh/SKILL.md
    pub fn install_skill_claude_code(server_url: &str) -> anyhow::Result<()> {
        let dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join("skills")
            .join("b0");
        fs::create_dir_all(&dir)?;

        let content = format!(
r#"---
name: b0
description: |
  Delegate tasks to AI workers via Box0. Use when the user asks to
  review code, check security, run tests, compare tools, get multiple
  perspectives, research a topic, analyze data, write docs, or any
  task that could benefit from specialized or parallel execution.
  Also use when the user mentions worker names or says "ask", "delegate",
  "get opinions from", or "have someone".
allowed-tools:
  - Bash
---

{body}"#,
            body = Self::skill_content(server_url)
        );

        fs::write(dir.join("SKILL.md"), content)?;
        Ok(())
    }

    pub fn uninstall_skill_claude_code() -> anyhow::Result<()> {
        let dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".claude")
            .join("skills")
            .join("b0");
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        // Clean up legacy bh.md
        let old = dir.with_extension("md");
        if old.exists() {
            let _ = fs::remove_file(&old);
        }
        Ok(())
    }

    /// Install skill for Codex: append marked section to ~/.codex/AGENTS.md
    pub fn install_skill_codex(server_url: &str) -> anyhow::Result<()> {
        let agents_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codex")
            .join("AGENTS.md");

        // Ensure directory exists
        if let Some(parent) = agents_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Read existing content and remove old box0 section if present
        let existing = fs::read_to_string(&agents_path).unwrap_or_default();
        let cleaned = Self::remove_marked_section(&existing);

        // Append new section
        let section = format!(
            "\n{}\n{}{}\n",
            Self::SKILL_MARKER_START,
            Self::skill_content(server_url),
            Self::SKILL_MARKER_END,
        );

        let new_content = format!("{}{}", cleaned.trim_end(), section);
        fs::write(&agents_path, new_content)?;
        Ok(())
    }

    pub fn uninstall_skill_codex() -> anyhow::Result<()> {
        let agents_path = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".codex")
            .join("AGENTS.md");

        if !agents_path.exists() {
            return Ok(());
        }

        let content = fs::read_to_string(&agents_path)?;
        let cleaned = Self::remove_marked_section(&content);
        let trimmed = cleaned.trim().to_string();

        if trimmed.is_empty() {
            fs::remove_file(&agents_path)?;
        } else {
            fs::write(&agents_path, format!("{}\n", trimmed))?;
        }
        Ok(())
    }

    fn remove_marked_section(content: &str) -> String {
        if let (Some(start), Some(end)) = (
            content.find(Self::SKILL_MARKER_START),
            content.find(Self::SKILL_MARKER_END),
        ) {
            let before = &content[..start];
            let after = &content[end + Self::SKILL_MARKER_END.len()..];
            format!("{}{}", before, after)
        } else {
            content.to_string()
        }
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
    pub group: String,
    pub task: String,
    pub created_at: String,
    #[serde(default)]
    pub temp: bool,
}
