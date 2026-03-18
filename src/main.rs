mod config;
mod db;

use axum::{
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{delete, get, post},
    Router,
};
use clap::Parser;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::signal;
use tower_http::cors::CorsLayer;

use config::Config;
use db::Database;

// --- App State ---

struct AppState {
    db: Database,
    api_keys: Vec<String>,
}

type SharedState = Arc<AppState>;

// --- CLI ---

#[derive(Parser)]
#[command(name = "stream0")]
struct Cli {
    #[arg(short, long)]
    config: Option<String>,
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
    aliases: Option<Vec<String>>,
}

#[derive(Deserialize)]
struct SendInboxRequest {
    task_id: String,
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
    task_id: Option<String>,
    #[serde(default)]
    timeout: Option<f64>,
}

// --- Auth Middleware ---

async fn auth_middleware(
    State(state): State<SharedState>,
    headers: HeaderMap,
    request: axum::extract::Request,
    next: Next,
) -> impl IntoResponse {
    if state.api_keys.is_empty() {
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
    let valid = state
        .api_keys
        .iter()
        .any(|k| key_bytes.ct_eq(k.as_bytes()).into());

    if !valid {
        return (
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({"error": "invalid API key"})),
        )
            .into_response();
    }

    next.run(request).await
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
) -> impl IntoResponse {
    match state.db.list_agents() {
        Ok(agents) => (StatusCode::OK, Json(serde_json::json!({"agents": agents}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn register_agent_handler(
    State(state): State<SharedState>,
    Json(req): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    match state.db.register_agent(&req.id, req.aliases.as_deref()) {
        Ok(agent) => (StatusCode::CREATED, Json(serde_json::to_value(agent).unwrap())).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn delete_agent_handler(
    State(state): State<SharedState>,
    Path(agent_id): Path<String>,
) -> impl IntoResponse {
    match state.db.delete_agent(&agent_id) {
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
    Path(agent_id): Path<String>,
    Json(req): Json<SendInboxRequest>,
) -> impl IntoResponse {
    // Resolve alias to canonical agent ID
    let resolved_id = match state.db.resolve_agent(&agent_id) {
        Ok(Some(id)) => id,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // Validate message type
    let valid_types = ["request", "question", "answer", "done", "failed"];
    if !valid_types.contains(&req.msg_type.as_str()) {
        return error_response(
            StatusCode::BAD_REQUEST,
            "type must be one of: request, question, answer, done, failed",
        );
    }

    match state.db.send_inbox_message(
        &req.task_id,
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
    Path(agent_id): Path<String>,
    Query(params): Query<InboxQuery>,
) -> impl IntoResponse {
    // Resolve alias to canonical agent ID
    let resolved_id = match state.db.resolve_agent(&agent_id) {
        Ok(Some(id)) => id,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // Track last_seen
    let _ = state.db.update_last_seen(&resolved_id);

    let timeout = params.timeout.unwrap_or(0.0).clamp(0.0, 30.0);
    let start = std::time::Instant::now();

    loop {
        match state.db.get_inbox_messages(
            &resolved_id,
            params.status.as_deref(),
            params.task_id.as_deref(),
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
    Path(message_id): Path<String>,
) -> impl IntoResponse {
    match state.db.ack_inbox_message(&message_id) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "acked", "message_id": message_id})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn get_task_messages_handler(
    State(state): State<SharedState>,
    Path(task_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_task_messages(&task_id) {
        Ok(messages) => (StatusCode::OK, Json(serde_json::json!({"messages": messages}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Helpers ---

fn error_response(status: StatusCode, message: &str) -> axum::response::Response {
    (status, Json(serde_json::json!({"error": message}))).into_response()
}

// --- Main ---

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let cfg = Config::load(cli.config.as_deref());

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

    if !cfg.auth.api_keys.is_empty() {
        tracing::info!(keys = cfg.auth.api_keys.len(), "API key authentication enabled");
    } else {
        tracing::warn!("No API keys configured - all endpoints are unauthenticated");
    }

    let state = Arc::new(AppState {
        db,
        api_keys: cfg.auth.api_keys.clone(),
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
        .route("/tasks/{task_id}/messages", get(get_task_messages_handler))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware));

    let app = Router::new()
        .merge(public)
        .merge(protected)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = cfg.address();
    tracing::info!(address = %addr, "Server starting");

    let listener = tokio::net::TcpListener::bind(&addr).await.expect("failed to bind");

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
