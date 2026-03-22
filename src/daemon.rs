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
/// Maps thread_id → Claude CLI session_id.
type Sessions = Arc<Mutex<HashMap<String, String>>>;

// --- Local daemon (runs inside server process, direct DB access) ---

pub async fn run_local(state: SharedState) {
    tracing::info!("Node daemon started (local)");

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS));
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);

    loop {
        // Get workers across ALL tenants on the local node
        let tenant_workers = match state.db.get_all_active_workers_for_node("local") {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("Failed to get workers: {}", e);
                tokio::time::sleep(poll_interval).await;
                continue;
            }
        };

        for (tenant, worker) in &tenant_workers {
            let messages = match state
                .db
                .get_inbox_messages(tenant, &worker.name, Some("unread"), None)
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
                let instructions = worker.instructions.clone();
                let sessions = sessions.clone();
                let msg = msg.clone();

                tokio::spawn(async move {
                    let _permit = permit;

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

                    tracing::info!(
                        worker = msg.to_agent,
                        thread = msg.thread_id,
                        resume = resume_session.is_some(),
                        "Processing task"
                    );

                    let result =
                        invoke_claude_cli(&instructions, &task_content, resume_session.as_deref())
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
                                worker = msg.to_agent,
                                thread = msg.thread_id,
                                "Task completed"
                            );
                            let _ = state.db.send_inbox_message(
                                &tenant,
                                &msg.thread_id,
                                &msg.to_agent,
                                &msg.from_agent,
                                "done",
                                Some(&serde_json::json!(output.text)),
                            );
                        }
                        Err(e) => {
                            tracing::error!(
                                worker = msg.to_agent,
                                thread = msg.thread_id,
                                error = %e,
                                "Task failed"
                            );
                            let _ = state.db.send_inbox_message(
                                &tenant,
                                &msg.thread_id,
                                &msg.to_agent,
                                &msg.from_agent,
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

// --- Remote daemon (runs on a remote node, uses HTTP client) ---

pub async fn run_remote(server_url: &str, node_id: &str, api_key: Option<&str>) {
    tracing::info!(node = node_id, "Node daemon started (remote)");

    let client = BhClient::new(server_url);
    if let Some(key) = api_key {
        client.set_api_key(key);
    }

    // Register node
    if let Err(e) = client.register_node(node_id).await {
        tracing::error!("Failed to register node: {}", e);
        return;
    }

    let semaphore = Arc::new(Semaphore::new(MAX_CONCURRENT_TASKS));
    let sessions: Sessions = Arc::new(Mutex::new(HashMap::new()));
    let poll_interval = Duration::from_millis(POLL_INTERVAL_MS);
    let heartbeat_interval = Duration::from_secs(30);
    let mut last_heartbeat = std::time::Instant::now();

    loop {
        // Periodic heartbeat
        if last_heartbeat.elapsed() >= heartbeat_interval {
            let _ = client.heartbeat_node(node_id).await;
            last_heartbeat = std::time::Instant::now();
        }

        // Get workers assigned to this node
        let workers = match client.list_workers_for_node(node_id).await {
            Ok(w) => w,
            Err(e) => {
                tracing::error!("Failed to get workers: {}", e);
                tokio::time::sleep(poll_interval).await;
                continue;
            }
        };

        for worker in &workers {
            // Poll worker's inbox
            let messages = match client
                .get_inbox(&worker.name, Some("unread"), Some(0.0))
                .await
            {
                Ok(m) => m,
                Err(_) => continue,
            };

            for msg in messages {
                if msg.msg_type != "request" && msg.msg_type != "answer" {
                    let _ = client.ack_message(&msg.id).await;
                    continue;
                }

                let _ = client.ack_message(&msg.id).await;

                let permit = match semaphore.clone().try_acquire_owned() {
                    Ok(p) => p,
                    Err(_) => break,
                };

                let client = client.clone();
                let instructions = worker.instructions.clone();
                let sessions = sessions.clone();
                let msg = msg.clone();

                tokio::spawn(async move {
                    let _permit = permit;

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

                    tracing::info!(
                        worker = msg.to_agent,
                        thread = msg.thread_id,
                        "Processing task"
                    );

                    let result =
                        invoke_claude_cli(&instructions, &task_content, resume_session.as_deref())
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
                                    &msg.from_agent,
                                    &msg.thread_id,
                                    &msg.to_agent,
                                    "done",
                                    Some(&serde_json::json!(output.text)),
                                )
                                .await;
                        }
                        Err(e) => {
                            let _ = client
                                .send_message(
                                    &msg.from_agent,
                                    &msg.thread_id,
                                    &msg.to_agent,
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

// --- Claude CLI invocation ---

struct ClaudeOutput {
    text: String,
    session_id: Option<String>,
}

async fn invoke_claude_cli(
    instructions: &str,
    task: &str,
    resume_session: Option<&str>,
) -> anyhow::Result<ClaudeOutput> {
    let mut cmd = tokio::process::Command::new("claude");
    cmd.args(["--print", "--output-format", "json"]);

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
            anyhow::anyhow!("claude CLI not found")
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

fn parse_claude_json(stdout: &str) -> anyhow::Result<ClaudeOutput> {
    match serde_json::from_str::<serde_json::Value>(stdout) {
        Ok(json) => {
            let text = json["result"]
                .as_str()
                .unwrap_or("(no result)")
                .to_string();
            let session_id = json["session_id"].as_str().map(|s| s.to_string());
            Ok(ClaudeOutput { text, session_id })
        }
        Err(_) => Ok(ClaudeOutput {
            text: stdout.to_string(),
            session_id: None,
        }),
    }
}
