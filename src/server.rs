use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Extension, Router,
};
use serde::Deserialize;
use std::sync::Arc;
use tokio::signal;
use tower_http::cors::CorsLayer;

use crate::config::ServerConfig;
use crate::daemon;
use crate::db::Database;

// --- Tenant ---

#[derive(Clone)]
pub struct Caller {
    /// "admin" or "member"
    pub role: String,
    /// Group name. Admin callers set this to the requested group.
    pub group: String,
    /// API key prefix identifying who made the request.
    pub key_prefix: String,
}

// --- App State ---

pub struct AppState {
    pub db: Database,
}

pub type SharedState = Arc<AppState>;

// --- Request/Response types ---

#[derive(Deserialize)]
struct RegisterAgentRequest {
    id: String,
    #[serde(default)]
    aliases: Option<Vec<String>>,
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
    instructions: String,
    #[serde(default = "default_node_id")]
    node_id: String,
}

fn default_node_id() -> String {
    "local".to_string()
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
struct GroupInviteRequest {
    group: String,
    #[serde(default)]
    description: Option<String>,
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

    match state.db.validate_api_key(key) {
        Ok(Some((role, group_name))) => {
            let prefix = key[..std::cmp::min(12, key.len())].to_string();
            // Admin keys default to "default" group; group keys use their group
            let group = group_name.unwrap_or_else(|| "default".to_string());
            request.extensions_mut().insert(Caller {
                role,
                group,
                key_prefix: prefix,
            });
            next.run(request).await
        }
        _ => (
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
        "version": "0.1.0"
    }))
}

// --- Handlers: Agents ---

async fn list_agents_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
) -> impl IntoResponse {
    match state.db.list_agents(&caller.group) {
        Ok(agents) => (StatusCode::OK, Json(serde_json::json!({"agents": agents}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn register_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    match state
        .db
        .register_agent(&caller.group, &req.id, req.aliases.as_deref())
    {
        Ok(agent) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(agent).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn delete_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_agent(&caller.group, &agent_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "deleted", "agent_id": agent_id})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

// --- Handlers: Inbox ---

async fn send_inbox_message_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(agent_id): Path<String>,
    Json(req): Json<SendInboxRequest>,
) -> impl IntoResponse {
    let resolved_id = match state.db.resolve_agent(&caller.group, &agent_id) {
        Ok(Some(id)) => id,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let valid_types = ["request", "question", "answer", "done", "failed", "message"];
    if !valid_types.contains(&req.msg_type.as_str()) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "type must be one of: request, question, answer, done, failed, message",
        );
    }

    match state.db.send_inbox_message(
        &caller.group,
        &req.thread_id,
        &req.from,
        &resolved_id,
        &req.msg_type,
        req.content.as_ref(),
    ) {
        Ok(msg) => (
            StatusCode::CREATED,
            Json(serde_json::json!({
                "message_id": msg.id,
                "created_at": msg.created_at,
            })),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_inbox_messages_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(agent_id): Path<String>,
    Query(params): Query<InboxQuery>,
) -> impl IntoResponse {
    let resolved_id = match state.db.resolve_agent(&caller.group, &agent_id) {
        Ok(Some(id)) => id,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let _ = state.db.update_last_seen(&caller.group, &resolved_id);

    let timeout = params.timeout.unwrap_or(0.0).clamp(0.0, 30.0);
    let start = std::time::Instant::now();

    loop {
        match state.db.get_inbox_messages(
            &caller.group,
            &resolved_id,
            params.status.as_deref(),
            params.thread_id.as_deref(),
        ) {
            Ok(messages) if !messages.is_empty() || timeout <= 0.0 => {
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({"messages": messages})),
                )
                    .into_response();
            }
            Ok(_) => {}
            Err(e) => {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
        }

        if start.elapsed().as_secs_f64() >= timeout {
            let empty: Vec<crate::db::InboxMessage> = vec![];
            return (
                StatusCode::OK,
                Json(serde_json::json!({"messages": empty})),
            )
                .into_response();
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
}

async fn ack_inbox_message_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(message_id): Path<String>,
) -> impl IntoResponse {
    match state.db.ack_inbox_message(&caller.group, &message_id) {
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
    Extension(caller): Extension<Caller>,
    Path(thread_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_thread_messages(&caller.group, &thread_id) {
        Ok(messages) => (
            StatusCode::OK,
            Json(serde_json::json!({"messages": messages})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Workers ---

async fn register_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<RegisterWorkerRequest>,
) -> impl IntoResponse {
    match state
        .db
        .register_worker(&caller.group, &req.name, &req.instructions, &req.node_id, &caller.key_prefix)
    {
        Ok(worker) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(worker).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_workers_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
) -> impl IntoResponse {
    match state.db.list_workers(&caller.group) {
        Ok(workers) => (
            StatusCode::OK,
            Json(serde_json::json!({"workers": workers})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.db.get_worker(&caller.group, &name) {
        Ok(Some(worker)) => {
            (StatusCode::OK, Json(serde_json::to_value(worker).unwrap())).into_response()
        }
        Ok(None) => error_response(StatusCode::NOT_FOUND, "worker not found"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn remove_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.db.remove_worker(&caller.group, &name, &caller.key_prefix) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "removed", "worker": name})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct UpdateWorkerRequest {
    instructions: String,
}

async fn update_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
    Json(req): Json<UpdateWorkerRequest>,
) -> impl IntoResponse {
    match state
        .db
        .update_worker_instructions(&caller.group, &name, &req.instructions, &caller.key_prefix)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "updated", "worker": name})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn stop_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state
        .db
        .set_worker_status(&caller.group, &name, "stopped", &caller.key_prefix)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "stopped", "worker": name})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn start_worker_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state
        .db
        .set_worker_status(&caller.group, &name, "active", &caller.key_prefix)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "active", "worker": name})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn worker_logs_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    match state.db.get_worker_logs(&caller.group, &name, 20) {
        Ok(messages) => (
            StatusCode::OK,
            Json(serde_json::json!({"messages": messages})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Nodes ---

async fn register_node_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<RegisterNodeRequest>,
) -> impl IntoResponse {
    match state.db.register_node(&caller.group, &req.id) {
        Ok(node) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(node).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_nodes_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
) -> impl IntoResponse {
    match state.db.list_nodes(&caller.group) {
        Ok(nodes) => (StatusCode::OK, Json(serde_json::json!({"nodes": nodes}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn remove_node_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match state.db.remove_node(&caller.group, &node_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "removed", "node": node_id})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn heartbeat_node_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(node_id): Path<String>,
) -> impl IntoResponse {
    match state.db.heartbeat_node(&caller.group, &node_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "ok"})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Groups ---

async fn create_group_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<CreateGroupRequest>,
) -> impl IntoResponse {
    if caller.role != "admin" {
        return error_response(StatusCode::FORBIDDEN, "admin key required");
    }
    match state.db.create_group(&req.name) {
        Ok(group) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(group).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_groups_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
) -> impl IntoResponse {
    if caller.role != "admin" {
        return error_response(StatusCode::FORBIDDEN, "admin key required");
    }
    match state.db.list_groups() {
        Ok(groups) => (
            StatusCode::OK,
            Json(serde_json::json!({"groups": groups})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn group_invite_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<GroupInviteRequest>,
) -> impl IntoResponse {
    if caller.role != "admin" {
        return error_response(StatusCode::FORBIDDEN, "admin key required");
    }
    let desc = req.description.unwrap_or_default();
    match state.db.create_group_key(&req.group, &desc) {
        Ok(full_key) => {
            let prefix = full_key[..std::cmp::min(12, full_key.len())].to_string();
            (
                StatusCode::CREATED,
                Json(serde_json::json!({
                    "key": full_key,
                    "key_prefix": prefix,
                    "group": req.group,
                })),
            )
                .into_response()
        }
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn list_keys_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
) -> impl IntoResponse {
    // Admin sees all keys; member sees own group's keys
    let filter = if caller.role == "admin" {
        None
    } else {
        Some(caller.group.as_str())
    };
    match state.db.list_api_keys(filter) {
        Ok(keys) => (StatusCode::OK, Json(serde_json::json!({"keys": keys}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn revoke_key_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(key_prefix): Path<String>,
) -> impl IntoResponse {
    if caller.role != "admin" {
        return error_response(StatusCode::FORBIDDEN, "admin key required");
    }
    match state.db.revoke_api_key(&key_prefix) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "revoked"})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

// --- Helpers ---

fn error_response(status: StatusCode, message: &str) -> axum::response::Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

// --- Server ---

pub async fn run(config: ServerConfig) {
    let db = Database::new(&config.db_path).expect("failed to initialize database");

    // Bootstrap admin key on first start
    match db.bootstrap_admin_key() {
        Ok(Some(key)) => {
            tracing::info!("Admin key generated (first start)");
            println!("\n  Admin key: {}\n", key);
            println!("  Save this key. Use it to login:");
            println!("  b0 login http://{}:{} --key {}\n", config.host, config.port, key);
        }
        Ok(None) => {}
        Err(e) => tracing::error!("Failed to bootstrap admin key: {}", e),
    }

    // Auto-register "local" node
    let _ = db.register_node("default", "local");

    let state = Arc::new(AppState { db });

    // Spawn daemon for "local" node
    let daemon_state = state.clone();
    tokio::spawn(async move {
        daemon::run_local(daemon_state).await;
    });

    // Public routes
    let public = Router::new().route("/health", get(health_handler));

    // Protected routes
    let protected = Router::new()
        // Agents
        .route(
            "/agents",
            get(list_agents_handler).post(register_agent_handler),
        )
        .route("/agents/{agent_id}", delete(delete_agent_handler))
        // Inbox
        .route(
            "/agents/{agent_id}/inbox",
            get(get_inbox_messages_handler).post(send_inbox_message_handler),
        )
        .route(
            "/inbox/messages/{message_id}/ack",
            post(ack_inbox_message_handler),
        )
        .route(
            "/threads/{thread_id}/messages",
            get(get_thread_messages_handler),
        )
        // Workers
        .route(
            "/workers",
            get(list_workers_handler).post(register_worker_handler),
        )
        .route(
            "/workers/{name}",
            get(get_worker_handler).delete(remove_worker_handler).put(update_worker_handler),
        )
        .route("/workers/{name}/stop", post(stop_worker_handler))
        .route("/workers/{name}/start", post(start_worker_handler))
        .route("/workers/{name}/logs", get(worker_logs_handler))
        // Nodes
        .route("/nodes", get(list_nodes_handler).post(register_node_handler))
        .route("/nodes/{node_id}", delete(remove_node_handler))
        .route("/nodes/{node_id}/heartbeat", post(heartbeat_node_handler))
        // Groups
        .route("/groups", get(list_groups_handler).post(create_group_handler))
        .route("/groups/invite", post(group_invite_handler))
        // Keys
        .route("/keys", get(list_keys_handler))
        .route("/keys/{key_prefix}", delete(revoke_key_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    let app = Router::new()
        .merge(public)
        .merge(protected)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = config.address();
    tracing::info!(address = %addr, "Box0 server starting");

    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .expect("failed to bind");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .expect("server error");

    tracing::info!("Box0 server stopped");
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
