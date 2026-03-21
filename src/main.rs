mod config;
mod db;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Extension, Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::signal;
use tower_http::cors::CorsLayer;

use config::Config;
use db::Database;

// --- Tenant ---

#[derive(Clone)]
struct Tenant(String);

// --- App State ---

struct AppState {
    db: Database,
    key_map: HashMap<String, String>,
}

type SharedState = Arc<AppState>;

// --- CLI ---

#[derive(Parser)]
#[command(name = "stream0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Path to YAML config file
    #[arg(short, long, global = true)]
    config: Option<String>,
}

#[derive(clap::Subcommand)]
enum Command {
    /// Start the Stream0 server (default if no subcommand)
    Serve,
    /// Agent management
    Agent {
        #[command(subcommand)]
        action: AgentAction,
    },
    /// Connect your Claude Code to this Stream0 instance
    Connect {
        /// Stream0 server URL
        #[arg(long, default_value = "http://localhost:8080")]
        url: String,
        /// Agent name for your Claude Code
        #[arg(long)]
        name: Option<String>,
    },
    /// Show server status and registered agents
    Status {
        /// Stream0 server URL
        #[arg(long, default_value = "http://localhost:8080")]
        url: String,
    },
}

#[derive(clap::Subcommand)]
enum AgentAction {
    /// Register an agent and launch a Claude Code worker
    Start {
        /// Agent name
        #[arg(long)]
        name: String,
        /// What this agent does
        #[arg(long, default_value = "")]
        description: String,
        /// Stream0 server URL
        #[arg(long, default_value = "http://localhost:8080")]
        url: String,
    },
}

// --- Request/Response types ---

#[derive(Deserialize)]
struct TopicCreateRequest {
    name: String,
    #[serde(default = "default_retention")]
    retention_days: i32,
}
fn default_retention() -> i32 { 7 }

#[derive(Deserialize)]
struct ProduceRequest {
    payload: serde_json::Value,
    #[serde(default)]
    headers: serde_json::Map<String, serde_json::Value>,
}

#[derive(Serialize)]
struct ProduceResponse {
    message_id: String,
    offset: i64,
    timestamp: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize)]
struct ConsumeQuery {
    group: Option<String>,
    #[serde(default = "default_max")]
    max: i32,
    #[serde(default = "default_timeout")]
    timeout: f64,
    #[serde(default = "default_visibility")]
    visibility_timeout: i32,
}
fn default_max() -> i32 { 10 }
fn default_timeout() -> f64 { 5.0 }
fn default_visibility() -> i32 { 30 }

#[derive(Deserialize)]
struct AckRequest {
    group: String,
}

#[derive(Deserialize)]
struct RequestReplyRequest {
    payload: serde_json::Value,
    #[serde(default)]
    headers: serde_json::Map<String, serde_json::Value>,
    #[serde(default = "default_rr_timeout")]
    timeout: f64,
}
fn default_rr_timeout() -> f64 { 30.0 }

#[derive(Deserialize)]
struct ReplyRequest {
    payload: serde_json::Value,
    #[serde(default)]
    headers: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    group: Option<String>,
}

#[derive(Deserialize)]
struct RegisterAgentRequest {
    id: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    aliases: Option<Vec<String>>,
    #[serde(default)]
    webhook: Option<String>,
}

#[derive(Deserialize)]
struct SendInboxRequest {
    thread_id: String,
    from: String,
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(default)]
    content: Option<serde_json::Value>,
}

#[derive(Deserialize)]
struct InboxQuery {
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    thread_id: Option<String>,
    #[serde(default)]
    timeout: Option<f64>,
}

// --- Auth Middleware ---

async fn auth_middleware(
    State(state): State<SharedState>,
    headers: HeaderMap,
    mut request: axum::extract::Request,
    next: Next,
) -> impl IntoResponse {
    if state.key_map.is_empty() {
        // No auth configured — use default tenant
        request.extensions_mut().insert(Tenant("default".to_string()));
        return next.run(request).await;
    }

    let key = headers
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    if key.is_empty() {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "missing X-API-Key header"})),
        )
            .into_response();
    }

    let key_bytes = key.as_bytes();
    // Find the matching key using constant-time comparison and extract its tenant
    let tenant = state
        .key_map
        .iter()
        .find(|(k, _)| key_bytes.ct_eq(k.as_bytes()).into())
        .map(|(_, tenant)| tenant.clone());

    match tenant {
        Some(t) => {
            request.extensions_mut().insert(Tenant(t));
            next.run(request).await
        }
        None => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid API key"})),
        )
            .into_response(),
    }
}

// --- Handlers: Health ---

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "version": "0.2.0-rust"
    }))
}

// --- Handlers: Topics ---

async fn list_topics_handler(State(state): State<SharedState>) -> impl IntoResponse {
    match state.db.list_topics() {
        Ok(topics) => (StatusCode::OK, Json(serde_json::to_value(topics).unwrap())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn create_topic_handler(
    State(state): State<SharedState>,
    Json(req): Json<TopicCreateRequest>,
) -> impl IntoResponse {
    match state.db.create_topic(&req.name, req.retention_days) {
        Ok(topic) => (StatusCode::CREATED, Json(serde_json::to_value(topic).unwrap())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_topic_handler(
    State(state): State<SharedState>,
    Path(topic_name): Path<String>,
) -> impl IntoResponse {
    match state.db.get_topic(&topic_name) {
        Ok(Some(topic)) => (StatusCode::OK, Json(serde_json::to_value(topic).unwrap())).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "Topic not found"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn produce_message_handler(
    State(state): State<SharedState>,
    Path(topic_name): Path<String>,
    Json(req): Json<ProduceRequest>,
) -> impl IntoResponse {
    let topic = match state.db.get_topic(&topic_name) {
        Ok(Some(t)) => t,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "Topic not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    match state.db.publish_message(topic.id, &req.payload, &req.headers) {
        Ok(msg) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(ProduceResponse {
                message_id: msg.id,
                offset: msg.offset,
                timestamp: msg.timestamp,
            }).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn consume_messages_handler(
    State(state): State<SharedState>,
    Path(topic_name): Path<String>,
    Query(params): Query<ConsumeQuery>,
) -> impl IntoResponse {
    let group = match &params.group {
        Some(g) => g.clone(),
        None => return error_response(StatusCode::BAD_REQUEST, "group is required"),
    };

    let topic = match state.db.get_topic(&topic_name) {
        Ok(Some(t)) => t,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "Topic not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let max = params.max.clamp(1, 100);
    let timeout = params.timeout.clamp(0.0, 30.0);
    let visibility = params.visibility_timeout.clamp(5, 300);
    let consumer_id = format!("consumer-{}", uuid::Uuid::new_v4());

    let start = std::time::Instant::now();
    loop {
        match state.db.claim_messages(topic.id, &group, &consumer_id, max, visibility) {
            Ok(messages) if !messages.is_empty() => {
                return (StatusCode::OK, Json(serde_json::json!({"messages": messages}))).into_response();
            }
            Ok(_) => {}
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }

        if start.elapsed().as_secs_f64() >= timeout {
            return (StatusCode::OK, Json(serde_json::json!({"messages": []}))).into_response();
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

async fn acknowledge_message_handler(
    State(state): State<SharedState>,
    Path(message_id): Path<String>,
    Json(req): Json<AckRequest>,
) -> impl IntoResponse {
    match state.db.acknowledge_message(&message_id, &req.group) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "acknowledged", "message_id": message_id})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

// --- Handlers: Request-Reply ---

async fn request_reply_handler(
    State(state): State<SharedState>,
    Path(topic_name): Path<String>,
    Json(req): Json<RequestReplyRequest>,
) -> impl IntoResponse {
    let topic = match state.db.get_topic(&topic_name) {
        Ok(Some(t)) => t,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "Topic not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let correlation_id = format!("corr-{}", &uuid::Uuid::new_v4().to_string()[..16]);
    let mut headers = req.headers;
    headers.insert("correlation_id".to_string(), serde_json::Value::String(correlation_id.clone()));

    let msg = match state.db.publish_message(topic.id, &req.payload, &headers) {
        Ok(m) => m,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let timeout = req.timeout.clamp(0.5, 300.0);
    let start = std::time::Instant::now();

    loop {
        match state.db.get_reply(&correlation_id) {
            Ok(Some(reply)) => {
                let _ = state.db.delete_reply(&correlation_id);
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "request_id": msg.id,
                        "correlation_id": correlation_id,
                        "reply": reply,
                    })),
                )
                    .into_response();
            }
            Ok(None) => {}
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }

        if start.elapsed().as_secs_f64() >= timeout {
            return (
                StatusCode::GATEWAY_TIMEOUT,
                Json(serde_json::json!({
                    "error": "request timed out waiting for reply",
                    "request_id": msg.id,
                    "correlation_id": correlation_id,
                })),
            )
                .into_response();
        }
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    }
}

async fn reply_handler(
    State(state): State<SharedState>,
    Path(message_id): Path<String>,
    Json(req): Json<ReplyRequest>,
) -> impl IntoResponse {
    let msg = match state.db.get_message(&message_id) {
        Ok(Some(m)) => m,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "message not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let correlation_id = match msg.headers.get("correlation_id").and_then(|v| v.as_str()) {
        Some(id) => id.to_string(),
        None => return error_response(StatusCode::BAD_REQUEST, "message has no correlation_id header"),
    };

    let mut reply_headers = req.headers;
    reply_headers.insert("correlation_id".to_string(), serde_json::Value::String(correlation_id.clone()));

    if let Err(e) = state.db.insert_reply(&correlation_id, &req.payload, &reply_headers) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }

    if let Some(group) = &req.group {
        let _ = state.db.acknowledge_message(&message_id, group);
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "reply sent",
            "correlation_id": correlation_id,
            "message_id": message_id,
        })),
    )
        .into_response()
}

// --- Handlers: Inbox ---

async fn list_agents_handler(
    State(state): State<SharedState>,
    Extension(Tenant(tenant)): Extension<Tenant>,
) -> impl IntoResponse {
    match state.db.list_agents(&tenant) {
        Ok(agents) => (StatusCode::OK, Json(serde_json::json!({"agents": agents}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn register_agent_handler(
    State(state): State<SharedState>,
    Extension(Tenant(tenant)): Extension<Tenant>,
    Json(req): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    match state.db.register_agent(&tenant, &req.id, req.aliases.as_deref(), req.webhook.as_deref(), req.description.as_deref()) {
        Ok(agent) => (StatusCode::CREATED, Json(serde_json::to_value(agent).unwrap())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn delete_agent_handler(
    State(state): State<SharedState>,
    Extension(Tenant(tenant)): Extension<Tenant>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_agent(&tenant, &agent_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "agent_id": agent_id})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn send_inbox_message_handler(
    State(state): State<SharedState>,
    Extension(Tenant(tenant)): Extension<Tenant>,
    Path(agent_id): Path<String>,
    Json(req): Json<SendInboxRequest>,
) -> impl IntoResponse {
    // Resolve alias to canonical agent ID
    let resolved_id = match state.db.resolve_agent(&tenant, &agent_id) {
        Ok(Some(id)) => id,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // Validate message type
    let valid_types = ["request", "question", "answer", "done", "failed", "message"];
    if !valid_types.contains(&req.msg_type.as_str()) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "type must be one of: request, question, answer, done, failed, message",
        );
    }

    match state.db.send_inbox_message(
        &tenant,
        &req.thread_id,
        &req.from,
        &resolved_id,
        &req.msg_type,
        req.content.as_ref(),
    ) {
        Ok(msg) => {
            // Fire webhook notification in the background (fire-and-forget)
            if let Ok(Some(webhook_url)) = state.db.get_agent_webhook(&tenant, &resolved_id) {
                let payload = serde_json::json!({
                    "event": "new_message",
                    "agent_id": resolved_id,
                    "message_id": msg.id,
                    "thread_id": req.thread_id,
                    "from": req.from,
                    "type": req.msg_type,
                });
                tokio::spawn(async move {
                    let client = reqwest::Client::builder()
                        .timeout(std::time::Duration::from_secs(10))
                        .build()
                        .unwrap();
                    let _ = client.post(&webhook_url).json(&payload).send().await;
                });
            }

            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "message_id": msg.id,
                    "created_at": msg.created_at,
                })),
            )
                .into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_inbox_messages_handler(
    State(state): State<SharedState>,
    Extension(Tenant(tenant)): Extension<Tenant>,
    Path(agent_id): Path<String>,
    Query(params): Query<InboxQuery>,
) -> impl IntoResponse {
    // Resolve alias to canonical agent ID
    let resolved_id = match state.db.resolve_agent(&tenant, &agent_id) {
        Ok(Some(id)) => id,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // Track last_seen
    let _ = state.db.update_last_seen(&tenant, &resolved_id);

    let timeout = params.timeout.unwrap_or(0.0).clamp(0.0, 30.0);
    let start = std::time::Instant::now();

    loop {
        match state.db.get_inbox_messages(
            &tenant,
            &resolved_id,
            params.status.as_deref(),
            params.thread_id.as_deref(),
        ) {
            Ok(messages) if !messages.is_empty() || timeout <= 0.0 => {
                return (StatusCode::OK, Json(serde_json::json!({"messages": messages}))).into_response();
            }
            Ok(_) => {}
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }

        if start.elapsed().as_secs_f64() >= timeout {
            let empty: Vec<db::InboxMessage> = vec![];
            return (StatusCode::OK, Json(serde_json::json!({"messages": empty}))).into_response();
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

async fn ack_inbox_message_handler(
    State(state): State<SharedState>,
    Extension(Tenant(tenant)): Extension<Tenant>,
    Path(message_id): Path<String>,
) -> impl IntoResponse {
    match state.db.ack_inbox_message(&tenant, &message_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "acked", "message_id": message_id})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn get_thread_messages_handler(
    State(state): State<SharedState>,
    Extension(Tenant(tenant)): Extension<Tenant>,
    Path(thread_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_thread_messages(&tenant, &thread_id) {
        Ok(messages) => (StatusCode::OK, Json(serde_json::json!({"messages": messages}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Helpers ---

fn error_response(status: StatusCode, message: &str) -> axum::response::Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

// --- CLI subcommands ---

fn mcp_config_json(url: &str, api_key: &str, agent_id: &str) -> serde_json::Value {
    serde_json::json!({
        "mcpServers": {
            "stream0-channel": {
                "command": "npx",
                "args": ["stream0-channel"],
                "env": {
                    "STREAM0_URL": url,
                    "STREAM0_API_KEY": api_key,
                    "STREAM0_AGENT_ID": agent_id
                }
            }
        }
    })
}

async fn cmd_agent_start(name: &str, description: &str, url: &str) {
    let api_key = std::env::var("STREAM0_API_KEY").unwrap_or_default();

    // Check server is running
    let health_url = format!("{}/health", url);
    if reqwest::get(&health_url).await.is_err() {
        eprintln!("Error: Stream0 server not reachable at {}", url);
        eprintln!("Start the server first: stream0");
        std::process::exit(1);
    }

    // Register agent
    let client = reqwest::Client::new();
    let mut body = serde_json::json!({"id": name});
    if !description.is_empty() {
        body["description"] = serde_json::Value::String(description.to_string());
    }
    let mut req = client.post(format!("{}/agents", url)).json(&body);
    if !api_key.is_empty() {
        req = req.header("X-API-Key", &api_key);
    }
    match req.send().await {
        Ok(resp) if resp.status().is_success() => {
            eprintln!("Agent \"{}\" registered", name);
        }
        Ok(resp) => {
            let text = resp.text().await.unwrap_or_default();
            eprintln!("Failed to register agent: {}", text);
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to register agent: {}", e);
            std::process::exit(1);
        }
    }

    // Create temporary MCP config
    let tmp_dir = std::env::temp_dir().join(format!("stream0-agent-{}", name));
    std::fs::create_dir_all(&tmp_dir).expect("failed to create temp dir");
    let mcp_config_path = tmp_dir.join("mcp.json");

    let mcp_config = mcp_config_json(url, &api_key, name);
    std::fs::write(&mcp_config_path, serde_json::to_string_pretty(&mcp_config).unwrap())
        .expect("failed to write MCP config");

    eprintln!("Listening for tasks...");

    let system_prompt = if description.is_empty() {
        format!("You are '{}', a worker agent on Stream0. Wait for incoming tasks from other agents. When you receive a task via the channel, do the work using your tools, then reply with the result.", name)
    } else {
        format!("You are '{}', a worker agent on Stream0. {}. Wait for incoming tasks from other agents. When you receive a task via the channel, do the work using your tools, then reply with the result. Stay focused on your specialty.", name, description)
    };

    let prompt = if description.is_empty() {
        format!("You are agent '{}' on Stream0. You are now online and listening for tasks. Wait for incoming messages.", name)
    } else {
        format!("You are agent '{}' on Stream0. {}. You are now online and listening for tasks. Wait for incoming messages.", name, description)
    };

    // Launch Claude Code
    let err = std::process::Command::new("claude")
        .arg("--mcp-config")
        .arg(&mcp_config_path)
        .arg("--system-prompt")
        .arg(&system_prompt)
        .arg("--allowedTools")
        .arg("mcp__stream0-channel__reply,mcp__stream0-channel__ack,Read,Glob,Grep,Bash,Edit,Write")
        .arg("-p")
        .arg(&prompt)
        .status();

    match err {
        Ok(status) => std::process::exit(status.code().unwrap_or(0)),
        Err(e) => {
            eprintln!("Failed to launch claude: {}", e);
            eprintln!("Make sure Claude Code CLI is installed: https://claude.ai/code");
            std::process::exit(1);
        }
    }
}

async fn cmd_connect(url: &str, name: Option<&str>) {
    let api_key = std::env::var("STREAM0_API_KEY").unwrap_or_default();
    let agent_id = match name {
        Some(n) => n.to_string(),
        None => format!("{}-claude", whoami::username()),
    };

    // Check server is running
    let health_url = format!("{}/health", url);
    if reqwest::get(&health_url).await.is_err() {
        eprintln!("Error: Stream0 server not reachable at {}", url);
        std::process::exit(1);
    }

    // Register self
    let client = reqwest::Client::new();
    let mut req = client.post(format!("{}/agents", url))
        .json(&serde_json::json!({"id": agent_id}));
    if !api_key.is_empty() {
        req = req.header("X-API-Key", &api_key);
    }
    let _ = req.send().await;

    let mcp_file = std::path::Path::new(".mcp.json");

    if mcp_file.exists() {
        let content = std::fs::read_to_string(mcp_file).unwrap_or_default();
        if content.contains("stream0-channel") {
            println!("Stream0 channel already configured in .mcp.json");
        } else {
            println!("Warning: .mcp.json exists. Add this to your mcpServers:\n");
            println!("  \"stream0-channel\": {{");
            println!("    \"command\": \"npx\",");
            println!("    \"args\": [\"stream0-channel\"],");
            println!("    \"env\": {{");
            println!("      \"STREAM0_URL\": \"{}\",", url);
            println!("      \"STREAM0_API_KEY\": \"{}\",", api_key);
            println!("      \"STREAM0_AGENT_ID\": \"{}\"", agent_id);
            println!("    }}");
            println!("  }}");
            return;
        }
    } else {
        let config = mcp_config_json(url, &api_key, &agent_id);
        std::fs::write(mcp_file, serde_json::to_string_pretty(&config).unwrap())
            .expect("failed to write .mcp.json");
        println!("Stream0 connected to Claude Code");
        println!("Config written to .mcp.json");
    }

    // Show available agents
    println!("\nAvailable agents:");
    let mut req = client.get(format!("{}/agents", url));
    if !api_key.is_empty() {
        req = req.header("X-API-Key", &api_key);
    }
    if let Ok(resp) = req.send().await {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(agents) = data["agents"].as_array() {
                let mut found = false;
                for a in agents {
                    let id = a["id"].as_str().unwrap_or("");
                    if id == agent_id { continue; }
                    let desc = a["description"].as_str().unwrap_or("(no description)");
                    println!("  - {}: {}", id, desc);
                    found = true;
                }
                if !found {
                    println!("  (none yet)");
                }
            }
        }
    }
}

async fn cmd_status(url: &str) {
    let api_key = std::env::var("STREAM0_API_KEY").unwrap_or_default();

    // Check server
    let health_url = format!("{}/health", url);
    match reqwest::get(&health_url).await {
        Ok(resp) => {
            if let Ok(data) = resp.json::<serde_json::Value>().await {
                let version = data["version"].as_str().unwrap_or("?");
                println!("Stream0 running at {} ({})", url, version);
            }
        }
        Err(_) => {
            eprintln!("Stream0 server not reachable at {}", url);
            std::process::exit(1);
        }
    }

    // List agents
    let client = reqwest::Client::new();
    let mut req = client.get(format!("{}/agents", url));
    if !api_key.is_empty() {
        req = req.header("X-API-Key", &api_key);
    }

    println!("\nRegistered agents:");
    if let Ok(resp) = req.send().await {
        if let Ok(data) = resp.json::<serde_json::Value>().await {
            if let Some(agents) = data["agents"].as_array() {
                if agents.is_empty() {
                    println!("  (none)");
                    return;
                }
                let now = chrono::Utc::now();
                for a in agents {
                    let id = a["id"].as_str().unwrap_or("?");
                    let desc = a["description"].as_str().unwrap_or("(no description)");
                    let status = match a["last_seen"].as_str() {
                        Some(ls) => {
                            if let Ok(seen) = chrono::DateTime::parse_from_rfc3339(ls) {
                                let diff = (now - seen.with_timezone(&chrono::Utc)).num_seconds();
                                if diff < 300 { "online " } else { "offline" }
                            } else {
                                "offline"
                            }
                        }
                        None => "offline",
                    };
                    println!("  {} {}: {}", status, id, desc);
                }
            }
        }
    }
}

// --- Main ---

#[tokio::main]
async fn main() {
    let cli = Cli::parse();

    match cli.command {
        Some(Command::Agent { action: AgentAction::Start { name, description, url } }) => {
            cmd_agent_start(&name, &description, &url).await;
        }
        Some(Command::Connect { url, name }) => {
            cmd_connect(&url, name.as_deref()).await;
        }
        Some(Command::Status { url }) => {
            cmd_status(&url).await;
        }
        Some(Command::Serve) | None => {
            run_server(cli.config.as_deref()).await;
        }
    }
}

async fn run_server(config_path: Option<&str>) {
    let cfg = Config::load(config_path);

    // Setup logging
    if cfg.log.format == "json" {
        tracing_subscriber::fmt().json().with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cfg.log.level)),
        ).init();
    } else {
        tracing_subscriber::fmt().with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(&cfg.log.level)),
        ).init();
    }

    tracing::info!("Stream0 starting");

    let db = Database::new(&cfg.database.path).expect("failed to initialize database");

    let key_map = cfg.auth.build_key_map();

    if cfg.auth.has_keys() {
        tracing::info!(keys = cfg.auth.total_keys(), "API key authentication enabled");
    } else {
        tracing::warn!("No API keys configured - all endpoints are unauthenticated");
    }

    let state = Arc::new(AppState {
        db,
        key_map,
    });

    // Public routes (no auth)
    let public = Router::new().route("/health", get(health_handler));

    // Protected routes
    let protected = Router::new()
        // Topics (legacy)
        .route("/topics", get(list_topics_handler).post(create_topic_handler))
        .route("/topics/{topic}", get(get_topic_handler))
        .route("/topics/{topic}/messages", get(consume_messages_handler).post(produce_message_handler))
        .route("/messages/{message_id}/ack", post(acknowledge_message_handler))
        .route("/topics/{topic}/request", post(request_reply_handler))
        .route("/messages/{message_id}/reply", post(reply_handler))
        // Inbox
        .route("/agents", get(list_agents_handler).post(register_agent_handler))
        .route("/agents/{agent_id}", delete(delete_agent_handler))
        .route("/agents/{agent_id}/inbox", get(get_inbox_messages_handler).post(send_inbox_message_handler))
        .route("/inbox/messages/{message_id}/ack", post(ack_inbox_message_handler))
        .route("/threads/{thread_id}/messages", get(get_thread_messages_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    let app = Router::new()
        .merge(public)
        .merge(protected)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = cfg.address();
    tracing::info!(address = %addr, "Server starting");

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error: could not bind to {} ({})", addr, e);
            if e.kind() == std::io::ErrorKind::AddrInUse {
                eprintln!("Another process is already using that port. Kill it or use a different port:");
                eprintln!("  STREAM0_SERVER_PORT=8081 stream0");
            }
            std::process::exit(1);
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    tracing::info!("Stream0 stopped");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("Shutdown signal received");
}
