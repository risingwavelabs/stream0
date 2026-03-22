use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{delete, get, post, put},
    Extension, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::signal;
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;

use crate::config::ServerConfig;
use crate::daemon;
use crate::db::{Database, User};

// --- Caller context (extracted by auth middleware) ---

#[derive(Clone)]
pub struct Caller {
    pub user: User,
}

// --- App State ---

pub struct AppState {
    pub db: Database,
}

pub type SharedState = Arc<AppState>;

// --- Request types ---

#[derive(Deserialize)]
struct RegisterAgentRequest {
    id: String,
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

#[derive(Deserialize)]
struct RegisterWorkerRequest {
    name: String,
    #[serde(default)]
    description: String,
    instructions: String,
    #[serde(default = "default_node_id")]
    node_id: String,
    #[serde(default = "default_runtime")]
    runtime: String,
}

fn default_node_id() -> String {
    "local".to_string()
}

fn default_runtime() -> String {
    "auto".to_string()
}

#[derive(Deserialize)]
struct UpdateWorkerRequest {
    instructions: String,
}

#[derive(Deserialize)]
struct RegisterNodeRequest {
    id: String,
}

#[derive(Deserialize)]
struct CreateGroupRequest {
    name: String,
}

#[derive(Deserialize)]
struct InviteRequest {
    name: String,
}

// --- Auth Middleware ---

async fn auth_middleware(
    State(state): State<SharedState>,
    headers: HeaderMap,
    mut request: axum::extract::Request,
    next: Next,
) -> impl IntoResponse {
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

    match state.db.authenticate(key) {
        Ok(Some(user)) => {
            request.extensions_mut().insert(Caller { user });
            next.run(request).await
        }
        _ => (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid API key"})),
        )
            .into_response(),
    }
}

/// Check caller is a member of the group. Returns error response if not.
fn require_group_member(
    state: &AppState,
    caller: &Caller,
    group_name: &str,
) -> Result<(), axum::response::Response> {
    if caller.user.is_admin {
        return Ok(());
    }
    match state.db.is_group_member(group_name, &caller.user.id) {
        Ok(true) => Ok(()),
        _ => Err(error_response(
            StatusCode::FORBIDDEN,
            "not a member of this group",
        )),
    }
}

// --- Handlers: Health ---

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "version": "0.1.0"
    }))
}

// --- Handlers: Agents ---

async fn register_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(group_name): Path<String>,
    Json(req): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.register_agent(&group_name, &req.id) {
        Ok(agent) => (StatusCode::CREATED, Json(serde_json::to_value(agent).unwrap())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Inbox ---

async fn send_inbox_message_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, agent_id)): Path<(String, String)>,
    Json(req): Json<SendInboxRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }

    let resolved_id = match state.db.resolve_agent(&group_name, &agent_id) {
        Ok(Some(id)) => id,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let valid_types = ["request", "question", "answer", "done", "failed", "message"];
    if !valid_types.contains(&req.msg_type.as_str()) {
        return error_response(StatusCode::BAD_REQUEST, "invalid message type");
    }

    match state.db.send_inbox_message(
        &group_name, &req.thread_id, &req.from, &resolved_id, &req.msg_type, req.content.as_ref(),
    ) {
        Ok(msg) => (
            StatusCode::CREATED,
            Json(serde_json::json!({"message_id": msg.id, "created_at": msg.created_at})),
        ).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_inbox_messages_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, agent_id)): Path<(String, String)>,
    Query(params): Query<InboxQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }

    let resolved_id = match state.db.resolve_agent(&group_name, &agent_id) {
        Ok(Some(id)) => id,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let _ = state.db.update_last_seen(&group_name, &resolved_id);

    let timeout = params.timeout.unwrap_or(0.0).clamp(0.0, 30.0);
    let start = std::time::Instant::now();

    loop {
        match state.db.get_inbox_messages(
            &group_name, &resolved_id, params.status.as_deref(), params.thread_id.as_deref(),
        ) {
            Ok(messages) if !messages.is_empty() || timeout <= 0.0 => {
                return (StatusCode::OK, Json(serde_json::json!({"messages": messages}))).into_response();
            }
            Ok(_) => {}
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }

        if start.elapsed().as_secs_f64() >= timeout {
            let empty: Vec<crate::db::InboxMessage> = vec![];
            return (StatusCode::OK, Json(serde_json::json!({"messages": empty}))).into_response();
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

async fn ack_inbox_message_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.ack_inbox_message(&group_name, &message_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "acked"}))).into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn get_thread_messages_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, thread_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.get_thread_messages(&group_name, &thread_id) {
        Ok(messages) => (StatusCode::OK, Json(serde_json::json!({"messages": messages}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Workers ---

async fn register_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(group_name): Path<String>,
    Json(req): Json<RegisterWorkerRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }

    // Check node ownership if not "local"
    if req.node_id != "local" {
        match state.db.get_node_owner(&req.node_id) {
            Ok(Some(owner)) if owner == caller.user.id => {}
            Ok(Some(_)) => {
                return error_response(StatusCode::FORBIDDEN, "you don't own this node");
            }
            Ok(None) => {
                return error_response(StatusCode::NOT_FOUND, "node not found");
            }
            Err(e) => {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
        }
    }

    match state.db.register_worker(&group_name, &req.name, &req.description, &req.instructions, &req.node_id, &req.runtime, &caller.user.id) {
        Ok(worker) => (StatusCode::CREATED, Json(serde_json::to_value(worker).unwrap())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_workers_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(group_name): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.list_workers(&group_name) {
        Ok(workers) => (StatusCode::OK, Json(serde_json::json!({"workers": workers}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.get_worker(&group_name, &name) {
        Ok(Some(worker)) => (StatusCode::OK, Json(serde_json::to_value(worker).unwrap())).into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "worker not found"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn remove_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.remove_worker(&group_name, &name, &caller.user.id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "removed"}))).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn update_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, name)): Path<(String, String)>,
    Json(req): Json<UpdateWorkerRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.update_worker_instructions(&group_name, &name, &req.instructions, &caller.user.id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "updated"}))).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn stop_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.set_worker_status(&group_name, &name, "stopped", &caller.user.id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "stopped"}))).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn start_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.set_worker_status(&group_name, &name, "active", &caller.user.id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "active"}))).into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn worker_logs_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.get_worker_logs(&group_name, &name, 20) {
        Ok(messages) => (StatusCode::OK, Json(serde_json::json!({"messages": messages}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Nodes ---

async fn register_node_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<RegisterNodeRequest>,
) -> impl IntoResponse {
    match state.db.register_node(&req.id, &caller.user.id) {
        Ok(node) => (StatusCode::CREATED, Json(serde_json::to_value(node).unwrap())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_nodes_handler(
    State(state): State<SharedState>,
) -> impl IntoResponse {
    match state.db.list_nodes() {
        Ok(nodes) => (StatusCode::OK, Json(serde_json::json!({"nodes": nodes}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn heartbeat_node_handler(
    State(state): State<SharedState>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match state.db.heartbeat_node(&node_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Groups ---

async fn create_group_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<CreateGroupRequest>,
) -> impl IntoResponse {
    match state.db.create_group(&req.name, &caller.user.id) {
        Ok(group) => (StatusCode::CREATED, Json(serde_json::to_value(group).unwrap())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_groups_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
) -> impl IntoResponse {
    match state.db.list_groups_for_user(&caller.user.id) {
        Ok(groups) => (StatusCode::OK, Json(serde_json::json!({"groups": groups}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Users (admin) ---

async fn invite_user_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<InviteRequest>,
) -> impl IntoResponse {
    if !caller.user.is_admin {
        return error_response(StatusCode::FORBIDDEN, "admin only");
    }
    match state.db.create_user(&req.name, false) {
        Ok((user, key)) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "user_id": user.id,
                "name": user.name,
                "key": key,
            })),
        ).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn add_to_group_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((group_name, user_id)): Path<(String, String)>,
) -> impl IntoResponse {
    // Must be group creator or admin
    if let Err(e) = require_group_member(&state, &caller, &group_name) {
        return e;
    }
    match state.db.add_group_member(&group_name, &user_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "added"}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

/// Returns all active workers on a node across all groups. Used by remote daemons.
async fn node_workers_handler(
    State(state): State<SharedState>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_all_active_workers_for_node(&node_id) {
        Ok(workers) => {
            let items: Vec<serde_json::Value> = workers
                .into_iter()
                .map(|(group, w)| {
                    let mut v = serde_json::to_value(&w).unwrap();
                    v["group"] = serde_json::Value::String(group);
                    v
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({"workers": items}))).into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_users_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
) -> impl IntoResponse {
    if !caller.user.is_admin {
        return error_response(StatusCode::FORBIDDEN, "admin only");
    }
    match state.db.list_users() {
        Ok(users) => (StatusCode::OK, Json(serde_json::json!({"users": users}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Helpers ---

fn error_response(status: StatusCode, message: &str) -> axum::response::Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

// --- Router ---

/// Build the Axum router with all routes. Extracted for use in tests.
pub fn build_router(state: SharedState) -> Router {
    let public = Router::new().route("/health", get(health_handler));

    let protected = Router::new()
        .route("/groups", get(list_groups_handler).post(create_group_handler))
        .route("/groups/{group_name}/members/{user_id}", post(add_to_group_handler))
        .route("/groups/{group_name}/agents", post(register_agent_handler))
        .route("/groups/{group_name}/agents/{agent_id}/inbox",
            get(get_inbox_messages_handler).post(send_inbox_message_handler))
        .route("/groups/{group_name}/inbox/{message_id}/ack", post(ack_inbox_message_handler))
        .route("/groups/{group_name}/threads/{thread_id}", get(get_thread_messages_handler))
        .route("/groups/{group_name}/workers",
            get(list_workers_handler).post(register_worker_handler))
        .route("/groups/{group_name}/workers/{name}",
            get(get_worker_handler).delete(remove_worker_handler).put(update_worker_handler))
        .route("/groups/{group_name}/workers/{name}/stop", post(stop_worker_handler))
        .route("/groups/{group_name}/workers/{name}/start", post(start_worker_handler))
        .route("/groups/{group_name}/workers/{name}/logs", get(worker_logs_handler))
        .route("/nodes", get(list_nodes_handler).post(register_node_handler))
        .route("/nodes/{node_id}/heartbeat", post(heartbeat_node_handler))
        .route("/nodes/{node_id}/workers", get(node_workers_handler))
        .route("/users", get(list_users_handler))
        .route("/users/invite", post(invite_user_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    let web_dir = std::path::Path::new("web");
    let serve_dir = ServeDir::new(web_dir)
        .fallback(tower_http::services::ServeFile::new(web_dir.join("index.html")));

    Router::new()
        .merge(public)
        .merge(protected)
        .fallback_service(serve_dir)
        .layer(CorsLayer::permissive())
        .with_state(state)
}

// --- Banner ---

const LOGO: &str = r#"
    ____   ____  _  _  ___
   | __ ) / __ \\ \/ // _ \
   |  _ \| |  | |\  /| | | |
   | |_) | |__| |/  \| |_| |
   |____/ \____//_/\_\\___/"#;

const BOX_WIDTH: usize = 62;

fn banner_line(text: &str) -> String {
    let len = text.len();
    let padding = if len < BOX_WIDTH - 2 {
        BOX_WIDTH - 2 - len
    } else {
        0
    };
    format!("│{}{}│", text, " ".repeat(padding))
}

fn banner_empty() -> String {
    banner_line(&" ".repeat(BOX_WIDTH - 2))
}

fn banner_top() -> String {
    format!("╭{}╮", "─".repeat(BOX_WIDTH - 2))
}

fn banner_bottom() -> String {
    format!("╰{}╯", "─".repeat(BOX_WIDTH - 2))
}

fn banner_separator() -> String {
    format!("├{}┤", "─".repeat(BOX_WIDTH - 2))
}

fn print_banner(
    server_url: &str,
    db_path: &str,
    workers_path: &str,
    first_start: Option<(&str, &str, &str)>, // (key, user_name, user_id)
) {
    let version = env!("CARGO_PKG_VERSION");
    let mut lines = Vec::new();

    lines.push(banner_top());
    // Logo
    for logo_line in LOGO.lines() {
        if logo_line.is_empty() { continue; }
        lines.push(banner_line(logo_line));
    }
    lines.push(banner_empty());
    lines.push(banner_line(&format!(
        "   v{:<28}{}",
        version, server_url
    )));
    lines.push(banner_empty());

    // First start section
    if let Some((key, user_name, user_id)) = first_start {
        lines.push(banner_separator());
        lines.push(banner_empty());
        lines.push(banner_line("   First start detected."));
        lines.push(banner_empty());
        lines.push(banner_line(&format!("   Admin key:  {}", key)));
        lines.push(banner_line(&format!("   User:       {} ({})", user_name, user_id)));
        lines.push(banner_empty());
        lines.push(banner_line("   CLI auto-configured. No login needed on this machine."));
        lines.push(banner_line("   Next step:  b0 worker add <name> --instructions \"...\""));
        lines.push(banner_empty());
    }

    // Info section
    lines.push(banner_separator());
    lines.push(banner_empty());
    lines.push(banner_line(&format!("   Database:   {}", db_path)));
    lines.push(banner_line(&format!("   Workers:    {}", workers_path)));
    lines.push(banner_line(&format!("   Dashboard:  {}", server_url)));
    lines.push(banner_empty());
    lines.push(banner_line("   Press Ctrl+C to stop."));
    lines.push(banner_empty());
    lines.push(banner_bottom());

    println!();
    for line in &lines {
        println!("{}", line);
    }
    println!();
}

// --- Server ---

pub async fn run(config: ServerConfig) {
    // Ensure DB parent directory exists
    if let Some(parent) = std::path::Path::new(&config.db_path).parent() {
        std::fs::create_dir_all(parent).expect("failed to create database directory");
    }

    let db = Database::new(&config.db_path).expect("failed to initialize database");

    let server_url = format!("http://{}:{}", config.host, config.port);

    // Derive workspace root from DB path
    let workspace_root = std::path::Path::new(&config.db_path)
        .parent()
        .unwrap_or(std::path::Path::new("."))
        .join("workers");
    let workers_display = format!("{}/", workspace_root.display());

    // Shorten paths for display: replace home dir with ~
    let db_display = match dirs::home_dir() {
        Some(home) => config.db_path.replace(&home.to_string_lossy().to_string(), "~"),
        None => config.db_path.clone(),
    };
    let workers_display = match dirs::home_dir() {
        Some(home) => workers_display.replace(&home.to_string_lossy().to_string(), "~"),
        None => workers_display,
    };

    // Bootstrap admin user on first start + auto-configure local CLI
    let first_start_info = match db.bootstrap_admin() {
        Ok(Some((user, key))) => {
            let mut cli_cfg = crate::config::CliConfig::load();
            cli_cfg.server_url = format!("http://127.0.0.1:{}", config.port);
            cli_cfg.api_key = Some(key.clone());
            cli_cfg.default_group = Some(user.name.clone());
            let _ = cli_cfg.lead_id();
            if let Err(e) = cli_cfg.save() {
                tracing::warn!("Failed to auto-configure CLI: {}", e);
            }
            Some((key, user.name.clone(), user.id.clone()))
        }
        Ok(None) => None,
        Err(e) => { tracing::error!("Failed to bootstrap admin: {}", e); None }
    };

    // Auto-register "local" node owned by admin
    if let Ok(Some(admin_id)) = db.get_admin_user_id() {
        let _ = db.register_node("local", &admin_id);
    }

    // Print banner
    print_banner(
        &server_url,
        &db_display,
        &workers_display,
        first_start_info.as_ref().map(|(k, n, i)| (k.as_str(), n.as_str(), i.as_str())),
    );

    let state = Arc::new(AppState { db });

    // Spawn daemon for "local" node
    let daemon_state = state.clone();
    tokio::spawn(async move {
        daemon::run_local(daemon_state, workspace_root).await;
    });

    let app = build_router(state);


    let addr = config.address();

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error: cannot bind to {}. {}", addr, e);
            if e.kind() == std::io::ErrorKind::AddrInUse {
                let port = addr.split(':').last().unwrap_or("8080");
                eprintln!("Hint: kill the existing process: kill $(lsof -ti :{})", port);
                eprintln!("  or use a different port:       b0 server --port <other>", );
            }
            std::process::exit(1);
        }
    };

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
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
