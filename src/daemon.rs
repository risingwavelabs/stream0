use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{Mutex, Semaphore};

use crate::client::BhClient;
use crate::server::SharedState;

const POLL_INTERVAL_MS: u64 = 2000;
const MAX_CONCURRENT_TASKS: usize = 4;
const TASK_TIMEOUT_SECS: u64 = 300;

/// Session tracker for multi-turn conversations.
/// Maps thread_id -> Claude CLI session_id.
type Sessions = Arc<Mutex<HashMap<String, String>>>;

// --- Local daemon (runs inside server process, direct DB access) ---

pub async fn run_local(state: SharedState, workspace_root: std::path::PathBuf) {
    tracing::info!(workspace = %workspace_root.display(), "Machine daemon started (local)");

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS));
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);
    let workspace_root = Arc::new(workspace_root);

    loop {
        // Get agents across ALL tenants on the local machine
        let tenant_agents = match state.db.get_all_active_agents_for_machine("local") {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Failed to get agents: {}", e);
                tokio::time::sleep(poll_interval).await;
                continue;
            }
        };

        for (tenant, agent) in &tenant_agents {
            let messages = match state
                .db
                .get_inbox_messages(tenant, &agent.name, Some("unread"), None)
            {
                Ok(m) => m,
                Err(_) => continue,
            };

            for msg in messages {
                if msg.msg_type != "request" && msg.msg_type != "answer" {
                    let _ = state.db.ack_inbox_message(tenant, &msg.id);
                    continue;
                }

                let _ = state.db.ack_inbox_message(tenant, &msg.id);

                let permit = match semaphore.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => {
                        tracing::debug!("Max concurrent tasks reached");
                        break;
                    }
                };

                let state = state.clone();
                let tenant = tenant.clone();
                let instructions = agent.instructions.clone();
                let agent_name = agent.name.clone();
                let agent_runtime = agent.runtime.clone();
                let workspace_root = workspace_root.clone();
                let sessions = sessions.clone();
                let msg = msg.clone();

                tokio::spawn(async move {
                    let _permit = permit;

                    // Create agent directory if needed
                    let agent_dir = workspace_root.join(&agent_name);
                    if let Err(e) = tokio::fs::create_dir_all(&agent_dir).await {
                        tracing::error!(agent = agent_name, error = %e, "Failed to create agent directory");
                        return;
                    }

                    let task_content = msg
                        .content
                        .as_ref()
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let resume_session = if msg.msg_type == "answer" {
                        sessions.lock().await.get(&msg.thread_id).cloned()
                    } else {
                        None
                    };

                    let resolved_rt = resolve_runtime(&agent_runtime);
                    tracing::info!(
                        agent = msg.to_id,
                        thread = msg.thread_id,
                        runtime = resolved_rt,
                        dir = %agent_dir.display(),
                        resume = resume_session.is_some(),
                        "Processing task"
                    );

                    // Notify lead that we started processing
                    let _ = state.db.send_inbox_message(
                        &tenant,
                        &msg.thread_id,
                        &msg.to_id,
                        &msg.from_id,
                        "started",
                        None,
                    );

                    let result =
                        invoke_runtime(&agent_runtime, &instructions, &task_content, resume_session.as_deref(), Some(&agent_dir))
                            .await;

                    match result {
                        Ok(output) => {
                            if let Some(sid) = &output.session_id {
                                sessions
                                    .lock()
                                    .await
                                    .insert(msg.thread_id.clone(), sid.clone());
                            }

                            tracing::info!(
                                agent = msg.to_id,
                                thread = msg.thread_id,
                                "Task completed"
                            );
                            let _ = state.db.send_inbox_message(
                                &tenant,
                                &msg.thread_id,
                                &msg.to_id,
                                &msg.from_id,
                                "done",
                                Some(&serde_json::json!(output.text)),
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                agent = msg.to_id,
                                thread = msg.thread_id,
                                error = %e,
                                "Task failed"
                            );
                            let _ = state.db.send_inbox_message(
                                &tenant,
                                &msg.thread_id,
                                &msg.to_id,
                                &msg.from_id,
                                "failed",
                                Some(&serde_json::json!(e.to_string())),
                            );
                        }
                    }
                });
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

// --- Remote daemon (runs on a remote machine, uses HTTP client) ---

pub async fn run_remote(server_url: &str, machine_id: &str, api_key: Option<&str>) {
    tracing::info!(machine = machine_id, "Machine daemon started (remote)");

    let client = match api_key {
        Some(key) => BhClient::with_api_key(server_url, key),
        None => BhClient::new(server_url),
    };

    // Register machine
    if let Err(e) = client.register_machine(machine_id).await {
        tracing::error!("Failed to register machine: {}", e);
        return;
    }

    // Create workspace root for remote agents
    let workspace_root = std::path::PathBuf::from(
        dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("."))
    ).join(".b0").join("agents");
    let workspace_root = Arc::new(workspace_root);

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS));
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);
    let heartbeat_interval = Duration::from_secs(30);
    let mut last_heartbeat = std::time::Instant::now();

    loop {
        // Periodic heartbeat
        if last_heartbeat.elapsed() >= heartbeat_interval {
            let _ = client.heartbeat_machine(machine_id).await;
            last_heartbeat = std::time::Instant::now();
        }

        let agents = match client.machine_agents(machine_id).await {
            Ok(a) => a,
            Err(e) => {
                tracing::error!("Failed to get agents: {}", e);
                tokio::time::sleep(poll_interval).await;
                continue;
            }
        };

        for (workspace, agent) in &agents {
            let messages = match client
                .get_inbox(workspace, &agent.name, Some("unread"), Some(0.0))
                .await
            {
                Ok(m) => m,
                Err(_) => continue,
            };

            for msg in messages {
                if msg.msg_type != "request" && msg.msg_type != "answer" {
                    let _ = client.ack_message(workspace, &msg.id).await;
                    continue;
                }

                let _ = client.ack_message(workspace, &msg.id).await;

                let permit = match semaphore.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => break,
                };

                let client = client.clone();
                let workspace = workspace.clone();
                let agent_name = agent.name.clone();
                let instructions = agent.instructions.clone();
                let agent_runtime = agent.runtime.clone();
                let workspace_root = workspace_root.clone();
                let sessions = sessions.clone();
                let msg = msg.clone();

                tokio::spawn(async move {
                    let _permit = permit;

                    // Create agent directory
                    let agent_dir = workspace_root.join(&agent_name);
                    if let Err(e) = tokio::fs::create_dir_all(&agent_dir).await {
                        tracing::error!(agent = agent_name, error = %e, "Failed to create agent directory");
                        return;
                    }

                    let task_content = msg
                        .content
                        .as_ref()
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    let resume_session = if msg.msg_type == "answer" {
                        sessions.lock().await.get(&msg.thread_id).cloned()
                    } else {
                        None
                    };

                    let resolved_rt = resolve_runtime(&agent_runtime);
                    tracing::info!(
                        agent = msg.to_id,
                        thread = msg.thread_id,
                        runtime = resolved_rt,
                        dir = %agent_dir.display(),
                        "Processing task"
                    );

                    // Notify lead that we started processing
                    let _ = client
                        .send_message(
                            &workspace,
                            &msg.from_id,
                            &msg.thread_id,
                            &msg.to_id,
                            "started",
                            None,
                        )
                        .await;

                    let result =
                        invoke_runtime(&agent_runtime, &instructions, &task_content, resume_session.as_deref(), Some(&agent_dir))
                            .await;

                    match result {
                        Ok(output) => {
                            if let Some(sid) = &output.session_id {
                                sessions
                                    .lock()
                                    .await
                                    .insert(msg.thread_id.clone(), sid.clone());
                            }

                            let _ = client
                                .send_message(
                                    &workspace,
                                    &msg.from_id,
                                    &msg.thread_id,
                                    &msg.to_id,
                                    "done",
                                    Some(&serde_json::json!(output.text)),
                                )
                                .await;
                        }
                        Err(e) => {
                            let _ = client
                                .send_message(
                                    &workspace,
                                    &msg.from_id,
                                    &msg.thread_id,
                                    &msg.to_id,
                                    "failed",
                                    Some(&serde_json::json!(e.to_string())),
                                )
                                .await;
                        }
                    }
                });
            }
        }

        tokio::time::sleep(poll_interval).await;
    }
}

// --- Runtime abstraction ---

struct RuntimeOutput {
    text: String,
    session_id: Option<String>,
}

/// Resolve which runtime to use. "auto" detects what's installed (claude preferred).
fn resolve_runtime(configured: &str) -> &str {
    if configured != "auto" {
        return configured;
    }
    // Auto-detect: prefer claude, fall back to codex
    if which("claude") {
        "claude"
    } else if which("codex") {
        "codex"
    } else {
        "claude" // will fail with a clear error at invocation time
    }
}

fn which(cmd: &str) -> bool {
    let check = if cfg!(windows) { "where" } else { "which" };
    std::process::Command::new(check)
        .arg(cmd)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn invoke_runtime(
    runtime: &str,
    instructions: &str,
    task: &str,
    resume_session: Option<&str>,
    working_dir: Option<&std::path::Path>,
) -> anyhow::Result<RuntimeOutput> {
    let resolved = resolve_runtime(runtime);
    match resolved {
        "codex" => invoke_codex_cli(instructions, task, working_dir).await,
        _ => invoke_claude_cli(instructions, task, resume_session, working_dir).await,
    }
}

// --- Claude CLI ---

async fn invoke_claude_cli(
    instructions: &str,
    task: &str,
    resume_session: Option<&str>,
    working_dir: Option<&std::path::Path>,
) -> anyhow::Result<RuntimeOutput> {
    let mut cmd = tokio::process::Command::new("claude");
    cmd.args(["--print", "--output-format", "json"]);

    if let Some(dir) = working_dir {
        cmd.current_dir(dir);
    }

    if let Some(session_id) = resume_session {
        cmd.args(["--resume", session_id]);
    } else {
        cmd.args(["--system-prompt", instructions]);
    }

    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("claude CLI not found. Install it or use --runtime codex")
        } else {
            anyhow::anyhow!("failed to spawn claude CLI: {}", e)
        }
    })?;

    if let Some(mut stdin) = child.stdin.take() {
        use tokio::io::AsyncWriteExt;
        stdin.write_all(task.as_bytes()).await?;
        drop(stdin);
    }

    let result =
        tokio::time::timeout(Duration::from_secs(TASK_TIMEOUT_SECS), child.wait_with_output())
            .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_claude_json(&stdout)
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = parse_claude_json(&stdout) {
                return Ok(parsed);
            }
            Err(anyhow::anyhow!("claude CLI failed: {}", stderr))
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("claude CLI error: {}", e)),
        Err(_) => Err(anyhow::anyhow!(
            "task timed out after {}s",
            TASK_TIMEOUT_SECS
        )),
    }
}

fn parse_claude_json(stdout: &str) -> anyhow::Result<RuntimeOutput> {
    match serde_json::from_str::<serde_json::Value>(stdout) {
        Ok(json) => {
            let text = json["result"]
                .as_str()
                .unwrap_or("(no result)")
                .to_string();
            let session_id = json["session_id"].as_str().map(|s| s.to_string());
            Ok(RuntimeOutput { text, session_id })
        }
        Err(_) => Ok(RuntimeOutput {
            text: stdout.to_string(),
            session_id: None,
        }),
    }
}

// --- Codex CLI ---

async fn invoke_codex_cli(
    instructions: &str,
    task: &str,
    working_dir: Option<&std::path::Path>,
) -> anyhow::Result<RuntimeOutput> {
    let prompt = format!("{}\n\n{}", instructions, task);

    let mut cmd = tokio::process::Command::new("codex");
    cmd.args(["exec", "--json", "--full-auto", "--skip-git-repo-check"]);

    if let Some(dir) = working_dir {
        cmd.args(["-C", &dir.to_string_lossy()]);
    }

    cmd.arg(&prompt);
    cmd.stdin(std::process::Stdio::null());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd.spawn().map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            anyhow::anyhow!("codex CLI not found. Install it or use --runtime claude")
        } else {
            anyhow::anyhow!("failed to spawn codex CLI: {}", e)
        }
    })?;

    let result =
        tokio::time::timeout(Duration::from_secs(TASK_TIMEOUT_SECS), child.wait_with_output())
            .await;

    match result {
        Ok(Ok(output)) if output.status.success() => {
            let stdout = String::from_utf8_lossy(&output.stdout);
            parse_codex_jsonl(&stdout)
        }
        Ok(Ok(output)) => {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(parsed) = parse_codex_jsonl(&stdout) {
                return Ok(parsed);
            }
            Err(anyhow::anyhow!("codex CLI failed: {}", stderr))
        }
        Ok(Err(e)) => Err(anyhow::anyhow!("codex CLI error: {}", e)),
        Err(_) => Err(anyhow::anyhow!(
            "task timed out after {}s",
            TASK_TIMEOUT_SECS
        )),
    }
}

fn parse_codex_jsonl(stdout: &str) -> anyhow::Result<RuntimeOutput> {
    // Codex --json outputs JSONL. Extract agent message text.
    let mut last_text = String::new();
    for line in stdout.lines() {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
            // item.completed with item.text (primary format)
            if json["type"].as_str() == Some("item.completed") {
                if let Some(text) = json["item"]["text"].as_str() {
                    last_text = text.to_string();
                }
            }
            // Also check output_text and content as fallbacks
            if let Some(text) = json["output_text"].as_str() {
                last_text = text.to_string();
            }
            if json["type"].as_str() == Some("message") {
                if let Some(content) = json["content"].as_str() {
                    last_text = content.to_string();
                }
            }
        }
    }
    if last_text.is_empty() {
        // Fallback: return raw stdout
        last_text = stdout.to_string();
    }
    Ok(RuntimeOutput {
        text: last_text,
        session_id: None, // Codex doesn't support session resume in the same way
    })
}
