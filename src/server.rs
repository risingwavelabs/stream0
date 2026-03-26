use axum::{
    Extension, Router,
    extract::{Path, Query, State},
    http::{HeaderMap, StatusCode},
    middleware::{self, Next},
    response::{IntoResponse, Json},
    routing::{delete, get, post},
};
use serde::Deserialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use tokio::signal;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::config::ServerConfig;
use crate::daemon;
use crate::db::{
    Database, User, WorkflowDefinition, WorkflowEdgeDraft, WorkflowNodeDraft, WorkflowRun,
    WorkflowStepRun,
};

// --- Caller context (extracted by auth middleware) ---

#[derive(Clone)]
pub struct Caller {
    pub user: User,
}

// --- App State ---

pub struct AppState {
    pub db: Database,
    /// Notifies the local daemon when new inbox messages arrive.
    pub inbox_notify: tokio::sync::Notify,
    pub slack_token: Option<String>,
}

pub type SharedState = Arc<AppState>;

// --- Request types ---

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
struct RegisterAgentRequest {
    name: String,
    #[serde(default)]
    description: String,
    instructions: String,
    #[serde(default = "default_machine_id")]
    machine_id: String,
    #[serde(default = "default_runtime")]
    runtime: String,
    #[serde(default = "default_kind_background")]
    kind: String,
    #[serde(default)]
    webhook_url: Option<String>,
    #[serde(default)]
    slack_channel: Option<String>,
}

fn default_machine_id() -> String {
    "local".to_string()
}

fn default_runtime() -> String {
    "auto".to_string()
}

fn default_kind_background() -> String {
    "background".to_string()
}

#[derive(Deserialize)]
struct UpdateAgentRequest {
    instructions: String,
}

#[derive(Deserialize)]
struct RegisterMachineRequest {
    id: String,
}

#[derive(Deserialize)]
struct CreateWorkspaceRequest {
    name: String,
}

#[derive(Deserialize)]
struct InviteRequest {
    name: String,
}

#[derive(Deserialize)]
struct CreateTaskRequest {
    title: String,
    #[serde(default)]
    parent_task_id: Option<String>,
}

#[derive(Deserialize)]
struct TaskMessageRequest {
    content: String,
}

#[derive(Clone, Deserialize)]
struct WorkflowNodeRequest {
    #[serde(default)]
    id: Option<String>,
    kind: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    prompt: String,
    #[serde(default)]
    agent_name: Option<String>,
    #[serde(default)]
    position_x: Option<f64>,
    #[serde(default)]
    position_y: Option<f64>,
}

#[derive(Clone, Deserialize)]
struct WorkflowEdgeRequest {
    #[serde(default)]
    id: Option<String>,
    source_node_id: String,
    target_node_id: String,
}

#[derive(Deserialize)]
struct CreateWorkflowRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_workflow_status")]
    status: String,
    #[serde(default)]
    nodes: Vec<WorkflowNodeRequest>,
    #[serde(default)]
    edges: Vec<WorkflowEdgeRequest>,
}

#[derive(Deserialize)]
struct UpdateWorkflowRequest {
    name: String,
    #[serde(default)]
    description: String,
    #[serde(default = "default_workflow_status")]
    status: String,
    #[serde(default)]
    nodes: Vec<WorkflowNodeRequest>,
    #[serde(default)]
    edges: Vec<WorkflowEdgeRequest>,
}

fn default_workflow_status() -> String {
    "draft".to_string()
}

#[derive(Deserialize)]
struct CreateWorkflowRunRequest {
    #[serde(default)]
    input: Option<String>,
}

#[derive(Deserialize)]
struct WorkflowRunsQuery {
    #[serde(default)]
    workflow_id: Option<String>,
    #[serde(default = "default_workflow_runs_limit")]
    limit: i64,
}

fn default_workflow_runs_limit() -> i64 {
    20
}

#[derive(Deserialize)]
struct WorkflowStepInputRequest {
    input: String,
}

#[derive(Deserialize)]
struct RetryStepQuery {
    /// "all" (default): reset this step and all descendants.
    /// "self": reset only this step.
    #[serde(default = "default_retry_scope")]
    scope: String,
}

fn default_retry_scope() -> String {
    "all".to_string()
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

/// Check caller is a member of the workspace. Returns error response if not.
fn require_workspace_member(
    state: &AppState,
    caller: &Caller,
    workspace_name: &str,
) -> Result<(), axum::response::Response> {
    if caller.user.is_admin {
        return Ok(());
    }
    match state
        .db
        .is_workspace_member(workspace_name, &caller.user.id)
    {
        Ok(true) => Ok(()),
        _ => Err(error_response(
            StatusCode::FORBIDDEN,
            "not a member of this workspace",
        )),
    }
}

fn internal_error<E: std::fmt::Display>(error: E) -> axum::response::Response {
    error_response(StatusCode::INTERNAL_SERVER_ERROR, &error.to_string())
}

pub(crate) async fn process_inbox_message_side_effects(
    state: &AppState,
    workspace_name: &str,
    thread_id: &str,
    msg_type: &str,
    content: Option<&serde_json::Value>,
) -> anyhow::Result<()> {
    // Auto-update task status based on message type.
    if msg_type == "done" || msg_type == "failed" {
        if let Ok(Some(task)) = state.db.get_task_by_thread(workspace_name, thread_id) {
            let status = if msg_type == "done" { "done" } else { "failed" };
            let result = content.and_then(|v| v.as_str());
            state
                .db
                .update_task_status(workspace_name, &task.id, status, result)?;
        }
    }
    if msg_type == "question" {
        if let Ok(Some(task)) = state.db.get_task_by_thread(workspace_name, thread_id) {
            state
                .db
                .update_task_status(workspace_name, &task.id, "needs_input", None)?;
        }
    }

    handle_workflow_thread_message(state, workspace_name, thread_id, msg_type, content).await?;
    state.inbox_notify.notify_waiters();
    Ok(())
}

// --- Handlers: Health ---

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({
        "status": "healthy",
        "version": env!("CARGO_PKG_VERSION")
    }))
}

fn json_content_as_text(content: Option<&serde_json::Value>) -> Option<String> {
    match content {
        Some(serde_json::Value::String(text)) => Some(text.clone()),
        Some(value) => Some(value.to_string()),
        None => None,
    }
}

// --- Handlers: Inbox ---

async fn send_inbox_message_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, agent_name)): Path<(String, String)>,
    Json(req): Json<SendInboxRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }

    let valid_types = ["request", "question", "answer", "done", "failed", "message", "started"];

    // Only verify agent exists for new requests. Response messages (done, failed,
    // started, answer) target the lead agent which is not in the agents table.
    let response_types = ["done", "failed", "started", "answer"];
    if !response_types.contains(&req.msg_type.as_str()) {
        match state.db.get_agent(&workspace_name, &agent_name) {
            Ok(Some(_)) => {}
            Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }
    }
    if !valid_types.contains(&req.msg_type.as_str()) {
        return error_response(StatusCode::BAD_REQUEST, "invalid message type");
    }

    match state.db.send_inbox_message(
        &workspace_name,
        &req.thread_id,
        &req.from,
        &agent_name,
        &req.msg_type,
        req.content.as_ref(),
    ) {
        Ok(msg) => {
            if let Err(err) = process_inbox_message_side_effects(
                &state,
                &workspace_name,
                &req.thread_id,
                &req.msg_type,
                req.content.as_ref(),
            )
            .await
            {
                return internal_error(err);
            }
            (
                StatusCode::CREATED,
                Json(serde_json::json!({"message_id": msg.id, "created_at": msg.created_at})),
            )
                .into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_inbox_messages_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, agent_name)): Path<(String, String)>,
    Query(params): Query<InboxQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }

    // Don't require agent to be registered - leads poll their inbox
    // without being registered as agents in the agents table.

    let timeout = params.timeout.unwrap_or(0.0).clamp(0.0, 30.0);
    let start = std::time::Instant::now();

    loop {
        match state.db.get_inbox_messages(
            &workspace_name,
            &agent_name,
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
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }

        let remaining = timeout - start.elapsed().as_secs_f64();
        if remaining <= 0.0 {
            let empty: Vec<crate::db::InboxMessage> = vec![];
            return (StatusCode::OK, Json(serde_json::json!({"messages": empty}))).into_response();
        }
        // Wait for a notification or remaining timeout, whichever comes first
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs_f64(remaining),
            state.inbox_notify.notified(),
        )
        .await;
    }
}

async fn ack_inbox_message_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, message_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.ack_inbox_message(&workspace_name, &message_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "acked"}))).into_response(),
        Err(e) => error_response(StatusCode::NOT_FOUND, &e.to_string()),
    }
}

async fn get_thread_messages_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, thread_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.get_thread_messages(&workspace_name, &thread_id) {
        Ok(messages) => (
            StatusCode::OK,
            Json(serde_json::json!({"messages": messages})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Agents ---

async fn register_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
    Json(req): Json<RegisterAgentRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }

    // Validate machine exists and caller owns it
    match state.db.get_machine_owner(&req.machine_id) {
        Ok(Some(owner)) if req.machine_id == "local" || owner == caller.user.id => {}
        Ok(Some(_)) => {
            return error_response(StatusCode::FORBIDDEN, "you don't own this machine");
        }
        Ok(None) if req.machine_id == "local" => {
            return error_response(StatusCode::BAD_REQUEST, "no local machine available. This server was started with --no-local. Register a machine first: b0 machine join <server-url> --name <name> --key <key>");
        }
        Ok(None) => {
            return error_response(StatusCode::NOT_FOUND, "machine not found");
        }
        Err(e) => {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
        }
    }

    match state.db.register_agent(
        &workspace_name,
        &req.name,
        &req.description,
        &req.instructions,
        &req.machine_id,
        &req.runtime,
        &caller.user.id,
        &req.kind,
        req.webhook_url.as_deref(),
        req.slack_channel.as_deref(),
    ) {
        Ok(agent) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(agent).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_agents_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.list_agents(&workspace_name) {
        Ok(agents) => (StatusCode::OK, Json(serde_json::json!({"agents": agents}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.get_agent(&workspace_name, &name) {
        Ok(Some(agent)) => {
            (StatusCode::OK, Json(serde_json::to_value(agent).unwrap())).into_response()
        }
        Ok(None) => error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_agent_threads_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, name)): Path<(String, String)>,
    Query(params): Query<ThreadsQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }

    match state.db.get_agent(&workspace_name, &name) {
        Ok(Some(_)) => {}
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "agent not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }

    match state
        .db
        .list_agent_threads(&workspace_name, &name, params.limit)
    {
        Ok(threads) => (
            StatusCode::OK,
            Json(serde_json::json!({ "threads": threads })),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn remove_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state
        .db
        .remove_agent(&workspace_name, &name, &caller.user.id)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "removed"})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn update_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, name)): Path<(String, String)>,
    Json(req): Json<UpdateAgentRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.update_agent_instructions(
        &workspace_name,
        &name,
        &req.instructions,
        &caller.user.id,
    ) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "updated"})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn stop_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state
        .db
        .set_agent_status(&workspace_name, &name, "stopped", &caller.user.id)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "stopped"})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn start_agent_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state
        .db
        .set_agent_status(&workspace_name, &name, "active", &caller.user.id)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "active"})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn agent_logs_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, name)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.get_agent_logs(&workspace_name, &name, 20) {
        Ok(messages) => (
            StatusCode::OK,
            Json(serde_json::json!({"messages": messages})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Machines ---

async fn register_machine_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<RegisterMachineRequest>,
) -> impl IntoResponse {
    match state.db.register_machine(&req.id, &caller.user.id) {
        Ok(machine) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(machine).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_machines_handler(State(state): State<SharedState>) -> impl IntoResponse {
    match state.db.list_machines() {
        Ok(machines) => (
            StatusCode::OK,
            Json(serde_json::json!({"machines": machines})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn heartbeat_machine_handler(
    State(state): State<SharedState>,
    Path(machine_id): Path<String>,
) -> impl IntoResponse {
    match state.db.heartbeat_machine(&machine_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "ok"}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Workspaces ---

async fn create_workspace_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Json(req): Json<CreateWorkspaceRequest>,
) -> impl IntoResponse {
    match state.db.create_workspace(&req.name, &caller.user.id) {
        Ok(workspace) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(workspace).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_workspaces_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
) -> impl IntoResponse {
    match state.db.list_workspaces_for_user(&caller.user.id) {
        Ok(workspaces) => (
            StatusCode::OK,
            Json(serde_json::json!({"workspaces": workspaces})),
        )
            .into_response(),
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
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn add_to_workspace_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, user_id)): Path<(String, String)>,
) -> impl IntoResponse {
    // Must be workspace creator or admin
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.add_workspace_member(&workspace_name, &user_id) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({"status": "added"}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

// --- Handlers: Cron Jobs ---

#[derive(Deserialize)]
struct CreateCronRequest {
    agent: String,
    schedule: String,
    task: String,
    #[serde(default)]
    end_date: Option<String>,
}

#[derive(Deserialize)]
struct UpdateCronRequest {
    #[serde(default)]
    enabled: Option<bool>,
}

async fn create_cron_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
    Json(req): Json<CreateCronRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    // Validate
    if req.task.trim().is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "task is required");
    }
    if crate::scheduler::parse_schedule_secs(&req.schedule).is_none() {
        return error_response(
            StatusCode::BAD_REQUEST,
            "invalid schedule. Use: 30s, 5m, 1h, 6h, 1d",
        );
    }
    match state.db.create_cron_job(
        &workspace_name,
        &req.agent,
        &req.schedule,
        &req.task,
        &caller.user.id,
        req.end_date.as_deref(),
    ) {
        Ok(job) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(job).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn list_cron_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.list_cron_jobs(&workspace_name) {
        Ok(jobs) => (StatusCode::OK, Json(serde_json::json!({"cron_jobs": jobs}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn remove_cron_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, cron_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    // Get cron job's agent before deleting, so we can clean up auto-created agents
    let cron_agent = state
        .db
        .list_cron_jobs(&workspace_name)
        .ok()
        .and_then(|jobs| jobs.into_iter().find(|j| j.id == cron_id))
        .map(|j| j.agent);

    match state
        .db
        .remove_cron_job(&workspace_name, &cron_id, &caller.user.id)
    {
        Ok(()) => {
            // Clean up auto-created cron agent
            if let Some(agent_name) = cron_agent {
                if let Ok(Some(agent)) = state.db.get_agent(&workspace_name, &agent_name) {
                    if agent.kind == "cron" {
                        let _ = state.db.remove_agent(&workspace_name, &agent_name, "");
                    }
                }
            }
            (
                StatusCode::OK,
                Json(serde_json::json!({"status": "removed"})),
            )
                .into_response()
        }
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn update_cron_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, cron_id)): Path<(String, String)>,
    Json(req): Json<UpdateCronRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    if let Some(enabled) = req.enabled {
        if let Err(e) = state
            .db
            .set_cron_enabled(&workspace_name, &cron_id, enabled)
        {
            return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
        }
    }
    (
        StatusCode::OK,
        Json(serde_json::json!({"status": "updated"})),
    )
        .into_response()
}

/// Returns all active agents on a machine across all workspaces. Used by remote daemons.
async fn machine_agents_handler(
    State(state): State<SharedState>,
    Path(machine_id): Path<String>,
) -> impl IntoResponse {
    match state.db.get_all_active_agents_for_machine(&machine_id) {
        Ok(agents) => {
            let items: Vec<serde_json::Value> = agents
                .into_iter()
                .map(|(workspace, a)| {
                    let mut v = serde_json::to_value(&a).unwrap();
                    v["workspace"] = serde_json::Value::String(workspace);
                    v
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!({"agents": items}))).into_response()
        }
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

#[derive(Deserialize)]
struct MachinePollQuery {
    #[serde(default)]
    timeout: Option<f64>,
}

/// Long-poll endpoint for remote daemons. Returns all unread request/answer
/// messages for agents on this machine. Holds the connection up to `timeout`
/// seconds (max 30) waiting for messages to arrive.
async fn machine_poll_handler(
    State(state): State<SharedState>,
    Path(machine_id): Path<String>,
    Query(params): Query<MachinePollQuery>,
) -> impl IntoResponse {
    let timeout = params.timeout.unwrap_or(0.0).clamp(0.0, 30.0);
    let start = std::time::Instant::now();

    loop {
        match state.db.get_unread_messages_for_machine(&machine_id) {
            Ok(messages) if !messages.is_empty() || timeout <= 0.0 => {
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({"messages": messages})),
                )
                    .into_response();
            }
            Ok(_) => {}
            Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        }

        let remaining = timeout - start.elapsed().as_secs_f64();
        if remaining <= 0.0 {
            let empty: Vec<crate::db::MachineInboxMessage> = vec![];
            return (StatusCode::OK, Json(serde_json::json!({"messages": empty}))).into_response();
        }
        let _ = tokio::time::timeout(
            std::time::Duration::from_secs_f64(remaining),
            state.inbox_notify.notified(),
        )
        .await;
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

// --- Task handlers ---

async fn create_task_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }

    if req.title.trim().is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "title is required");
    }

    let thread_id = format!("thread-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let from_id = format!("web-{}", caller.user.id);

    // Check that "local" machine exists (it won't if server was started with --no-local)
    if let Ok(None) = state.db.get_machine_owner("local") {
        return error_response(StatusCode::BAD_REQUEST, "no local machine available. This server was started with --no-local. Register a machine first: b0 machine join <server-url> --name <name> --key <key>");
    }

    // Auto-create a temp agent for this task
    let agent_name = format!("task-{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let default_instructions = "You are a helpful assistant. Complete the task. Be concise.";
    if let Err(e) = state.db.register_agent(
        &workspace_name,
        &agent_name,
        "",
        default_instructions,
        "local",
        "auto",
        &caller.user.id,
        "temp",
        None,
        None,
    ) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }

    let task = match state.db.create_task(
        &workspace_name,
        &req.title,
        &agent_name,
        &thread_id,
        req.parent_task_id.as_deref(),
        &caller.user.id,
    ) {
        Ok(t) => t,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    // Send inbox message to task agent
    if let Err(e) = state.db.send_inbox_message(
        &workspace_name,
        &thread_id,
        &from_id,
        &agent_name,
        "request",
        Some(&serde_json::json!(req.title)),
    ) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    state.inbox_notify.notify_waiters();

    (
        StatusCode::CREATED,
        Json(serde_json::to_value(&task).unwrap()),
    )
        .into_response()
}

async fn list_tasks_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.list_tasks(&workspace_name) {
        Ok(tasks) => (StatusCode::OK, Json(serde_json::json!({"tasks": tasks}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_task_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, task_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    let task = match state.db.get_task(&workspace_name, &task_id) {
        Ok(Some(t)) => t,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "task not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let messages = state
        .db
        .get_thread_messages(&workspace_name, &task.thread_id)
        .unwrap_or_default();
    let subtasks = state
        .db
        .get_subtasks(&workspace_name, &task_id)
        .unwrap_or_default();

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "task": task,
            "messages": messages,
            "subtasks": subtasks,
        })),
    )
        .into_response()
}

async fn send_task_message_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, task_id)): Path<(String, String)>,
    Json(req): Json<TaskMessageRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }

    if req.content.trim().is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "content is required");
    }

    let task = match state.db.get_task(&workspace_name, &task_id) {
        Ok(Some(t)) => t,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "task not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let from_id = format!("web-{}", caller.user.id);

    // Send as "answer" to resume the agent's session
    if let Err(e) = state.db.send_inbox_message(
        &workspace_name,
        &task.thread_id,
        &from_id,
        &task.agent_name,
        "answer",
        Some(&serde_json::json!(req.content)),
    ) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    state.inbox_notify.notify_waiters();

    // Set task status back to running
    let _ = state
        .db
        .update_task_status(&workspace_name, &task_id, "running", None);

    (StatusCode::OK, Json(serde_json::json!({"status": "sent"}))).into_response()
}

// --- Workflow handlers ---

fn default_node_title(kind: &str) -> &'static str {
    match kind {
        "start" => "Start",
        "agent" => "Agent Step",
        "human_input" => "Human Input",
        "end" => "End",
        _ => "Step",
    }
}

fn validate_workflow_status(status: &str) -> bool {
    matches!(status, "draft" | "published" | "archived")
}

fn validate_workflow_definition(
    state: &AppState,
    workspace_name: &str,
    nodes: &[WorkflowNodeRequest],
    edges: &[WorkflowEdgeRequest],
) -> Result<(Vec<WorkflowNodeDraft>, Vec<WorkflowEdgeDraft>), axum::response::Response> {
    if nodes.is_empty() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "workflow must contain at least one node",
        ));
    }

    let mut drafts = Vec::with_capacity(nodes.len());
    let mut node_ids = HashSet::new();
    let mut start_count = 0;
    let mut agent_count = 0;

    for node in nodes {
        let kind = node.kind.trim().to_string();
        if !matches!(kind.as_str(), "start" | "agent" | "human_input" | "end") {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "invalid workflow node kind",
            ));
        }

        let id = node
            .id
            .clone()
            .unwrap_or_else(|| format!("node-{}", &uuid::Uuid::new_v4().to_string()[..8]));
        if !node_ids.insert(id.clone()) {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "duplicate workflow node id",
            ));
        }

        let title = if node.title.trim().is_empty() {
            default_node_title(&kind).to_string()
        } else {
            node.title.trim().to_string()
        };

        let agent_name = if kind == "agent" {
            let Some(agent_name) = node
                .agent_name
                .as_ref()
                .map(|value| value.trim())
                .filter(|value| !value.is_empty())
            else {
                return Err(error_response(
                    StatusCode::BAD_REQUEST,
                    "agent nodes must bind to an existing agent",
                ));
            };
            match state.db.get_agent(workspace_name, agent_name) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    return Err(error_response(
                        StatusCode::BAD_REQUEST,
                        "workflow references an unknown agent",
                    ));
                }
                Err(e) => {
                    return Err(error_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &e.to_string(),
                    ));
                }
            }
            agent_count += 1;
            Some(agent_name.to_string())
        } else {
            None
        };

        if kind == "start" {
            start_count += 1;
        }

        drafts.push(WorkflowNodeDraft {
            id,
            kind,
            title,
            prompt: node.prompt.trim().to_string(),
            agent_name,
            position_x: node.position_x.unwrap_or(0.0),
            position_y: node.position_y.unwrap_or(0.0),
        });
    }

    if start_count != 1 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "workflow must contain exactly one start node",
        ));
    }
    if agent_count == 0 {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "workflow must contain at least one agent node",
        ));
    }

    let start_node_id = drafts
        .iter()
        .find(|node| node.kind == "start")
        .map(|node| node.id.clone())
        .unwrap_or_default();

    let mut adjacency: HashMap<String, Vec<String>> = drafts
        .iter()
        .map(|node| (node.id.clone(), Vec::new()))
        .collect();
    let mut indegree: HashMap<String, usize> = drafts
        .iter()
        .map(|node| (node.id.clone(), 0usize))
        .collect();
    let mut edge_pairs = HashSet::new();
    let mut edge_drafts = Vec::with_capacity(edges.len());

    for edge in edges {
        let source = edge.source_node_id.trim();
        let target = edge.target_node_id.trim();

        if source.is_empty() || target.is_empty() {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "workflow edges require source and target node ids",
            ));
        }
        if source == target {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "workflow edges cannot connect a node to itself",
            ));
        }
        if !node_ids.contains(source) || !node_ids.contains(target) {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "workflow edge references an unknown node",
            ));
        }
        if !edge_pairs.insert((source.to_string(), target.to_string())) {
            return Err(error_response(
                StatusCode::BAD_REQUEST,
                "duplicate workflow edge",
            ));
        }

        adjacency
            .entry(source.to_string())
            .or_default()
            .push(target.to_string());
        *indegree.entry(target.to_string()).or_insert(0) += 1;

        edge_drafts.push(WorkflowEdgeDraft {
            id: edge
                .id
                .clone()
                .unwrap_or_else(|| format!("edge-{}", &uuid::Uuid::new_v4().to_string()[..8])),
            source_node_id: source.to_string(),
            target_node_id: target.to_string(),
        });
    }

    let mut reachable = HashSet::new();
    let mut queue = VecDeque::from([start_node_id.clone()]);
    while let Some(node_id) = queue.pop_front() {
        if !reachable.insert(node_id.clone()) {
            continue;
        }
        if let Some(neighbors) = adjacency.get(&node_id) {
            for neighbor in neighbors {
                queue.push_back(neighbor.clone());
            }
        }
    }

    if reachable.len() != drafts.len() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "every workflow node must be reachable from the start node",
        ));
    }

    let mut topo_queue: VecDeque<String> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(node_id, _)| node_id.clone())
        .collect();
    let mut visited = 0usize;
    let mut indegree_mut = indegree;
    while let Some(node_id) = topo_queue.pop_front() {
        visited += 1;
        if let Some(neighbors) = adjacency.get(&node_id) {
            for neighbor in neighbors {
                if let Some(degree) = indegree_mut.get_mut(neighbor) {
                    *degree -= 1;
                    if *degree == 0 {
                        topo_queue.push_back(neighbor.clone());
                    }
                }
            }
        }
    }

    if visited != drafts.len() {
        return Err(error_response(
            StatusCode::BAD_REQUEST,
            "workflow must be a DAG",
        ));
    }

    Ok((drafts, edge_drafts))
}

fn upstream_node_ids(definition: &WorkflowDefinition, node_id: &str) -> Vec<String> {
    definition
        .edges
        .iter()
        .filter(|edge| edge.target_node_id == node_id)
        .map(|edge| edge.source_node_id.clone())
        .collect()
}

fn descendant_node_ids(definition: &WorkflowDefinition, node_id: &str) -> HashSet<String> {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([node_id.to_string()]);
    while let Some(current) = queue.pop_front() {
        if !seen.insert(current.clone()) {
            continue;
        }
        for edge in definition
            .edges
            .iter()
            .filter(|edge| edge.source_node_id == current)
        {
            queue.push_back(edge.target_node_id.clone());
        }
    }
    seen
}

fn compose_workflow_step_input(
    definition: &WorkflowDefinition,
    run: &WorkflowRun,
    step: &WorkflowStepRun,
    step_by_node_id: &HashMap<String, WorkflowStepRun>,
) -> String {
    let upstream_outputs = upstream_node_ids(definition, &step.node_id)
        .into_iter()
        .filter_map(|node_id| step_by_node_id.get(&node_id))
        .filter_map(|upstream| {
            upstream
                .output
                .as_ref()
                .or(upstream.input.as_ref())
                .map(|text| format!("{}:\n{}", upstream.node_title, text))
        })
        .collect::<Vec<_>>()
        .join("\n\n");

    let run_input = run.input.as_deref().unwrap_or("");
    let prompt = if step.node_prompt.trim().is_empty() {
        "Complete this step.".to_string()
    } else {
        step.node_prompt.clone()
    };

    format!(
        "Workflow: {}\nStep: {}\n\nRun input:\n{}\n\nUpstream outputs:\n{}\n\nStep instructions:\n{}",
        definition.workflow.name,
        step.node_title,
        if run_input.is_empty() {
            "(none)"
        } else {
            run_input
        },
        if upstream_outputs.is_empty() {
            "(none)".to_string()
        } else {
            upstream_outputs
        },
        prompt,
    )
}

fn compute_workflow_run_state(step_runs: &[WorkflowStepRun]) -> (&'static str, Option<String>) {
    let has_failed = step_runs.iter().any(|step| step.status == "failed");
    let has_running = step_runs.iter().any(|step| step.status == "running");
    let has_ready = step_runs.iter().any(|step| step.status == "ready");
    let has_pending = step_runs.iter().any(|step| step.status == "pending");
    let has_waiting = step_runs
        .iter()
        .any(|step| step.status == "waiting_for_input");
    let all_terminal = step_runs
        .iter()
        .all(|step| matches!(step.status.as_str(), "done" | "skipped" | "failed"));

    if all_terminal {
        if has_failed {
            let error = step_runs
                .iter()
                .find(|step| step.status == "failed")
                .and_then(|step| step.error.clone().or_else(|| step.output.clone()));
            return ("failed", error);
        }
        return ("done", None);
    }
    // Still in progress: some steps are running/ready/pending alongside a failure
    if has_waiting {
        return ("waiting_for_input", None);
    }
    if has_running || has_ready {
        return ("running", None);
    }
    if has_pending {
        return ("queued", None);
    }
    ("queued", None)
}

fn workflow_run_response(
    state: &AppState,
    workspace_name: &str,
    run_id: &str,
) -> Result<serde_json::Value, axum::response::Response> {
    let run = match state.db.get_workflow_run(workspace_name, run_id) {
        Ok(Some(run)) => run,
        Ok(None) => {
            return Err(error_response(
                StatusCode::NOT_FOUND,
                "workflow run not found",
            ));
        }
        Err(e) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    };
    let workflow = match state.db.get_workflow(&run.workspace_name, &run.workflow_id) {
        Ok(Some(workflow)) => workflow,
        Ok(None) => return Err(error_response(StatusCode::NOT_FOUND, "workflow not found")),
        Err(e) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    };
    let step_runs = match state.db.list_workflow_step_runs(&run.id) {
        Ok(steps) => steps,
        Err(e) => {
            return Err(error_response(
                StatusCode::INTERNAL_SERVER_ERROR,
                &e.to_string(),
            ));
        }
    };

    Ok(serde_json::json!({
        "workflow": workflow,
        "run": run,
        "step_runs": step_runs,
    }))
}

async fn advance_workflow_run(
    state: &AppState,
    workspace_name: &str,
    run_id: &str,
) -> anyhow::Result<()> {
    let run = match state.db.get_workflow_run(workspace_name, run_id) {
        Ok(Some(run)) => run,
        Ok(None) => anyhow::bail!("workflow run not found"),
        Err(e) => return Err(e.into()),
    };
    // Prefer the snapshotted definition from run creation time.
    // Fall back to the current definition for runs created before snapshots were added.
    let definition = if let Some(snapshot) = run.definition_snapshot.as_deref() {
        serde_json::from_str::<WorkflowDefinition>(snapshot)
            .ok()
            .or_else(|| {
                state
                    .db
                    .get_workflow_definition(workspace_name, &run.workflow_id)
                    .ok()
                    .flatten()
            })
    } else {
        state
            .db
            .get_workflow_definition(workspace_name, &run.workflow_id)
            .ok()
            .flatten()
    };
    let definition = match definition {
        Some(d) => d,
        None => anyhow::bail!("workflow not found"),
    };

    loop {
        let step_runs = match state.db.list_workflow_step_runs(run_id) {
            Ok(steps) => steps,
            Err(e) => return Err(e.into()),
        };
        let step_by_node_id: HashMap<String, WorkflowStepRun> = step_runs
            .iter()
            .cloned()
            .map(|step| (step.node_id.clone(), step))
            .collect();

        enum Action {
            Dispatch {
                step: WorkflowStepRun,
                input: String,
            },
            WaitForInput {
                step: WorkflowStepRun,
            },
            Complete {
                step: WorkflowStepRun,
                output: String,
            },
        }

        let mut actions = Vec::new();
        for step in &step_runs {
            if !matches!(step.status.as_str(), "pending" | "ready") {
                continue;
            }
            let upstream = upstream_node_ids(&definition, &step.node_id);
            let deps_met = upstream.iter().all(|node_id| {
                step_by_node_id
                    .get(node_id)
                    .map(|upstream_step| upstream_step.status == "done")
                    .unwrap_or(false)
            });
            if !deps_met {
                continue;
            }

            match step.node_kind.as_str() {
                "agent" => {
                    actions.push(Action::Dispatch {
                        step: step.clone(),
                        input: compose_workflow_step_input(
                            &definition,
                            &run,
                            step,
                            &step_by_node_id,
                        ),
                    });
                }
                "human_input" => actions.push(Action::WaitForInput { step: step.clone() }),
                "end" => {
                    let output = upstream
                        .iter()
                        .filter_map(|node_id| step_by_node_id.get(node_id))
                        .filter_map(|upstream_step| {
                            upstream_step
                                .output
                                .as_ref()
                                .or(upstream_step.input.as_ref())
                                .map(|text| format!("{}: {}", upstream_step.node_title, text))
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    actions.push(Action::Complete {
                        step: step.clone(),
                        output,
                    });
                }
                _ => {}
            }
        }

        if actions.is_empty() {
            break;
        }

        for action in actions {
            match action {
                Action::Dispatch { step, input } => {
                    let Some(agent_name) = step.agent_name.as_deref() else {
                        anyhow::bail!("workflow step is missing an agent binding");
                    };
                    // Mark ready before dispatching so UI can see the transition
                    if step.status == "pending" {
                        let _ = state.db.mark_workflow_step_run_ready(&step.id);
                    }
                    let thread_id = format!("wft-{}", &uuid::Uuid::new_v4().to_string()[..8]);
                    if let Err(e) = state
                        .db
                        .dispatch_workflow_step_run(run_id, &step.id, &thread_id, &input)
                    {
                        return Err(e.into());
                    }
                    if let Err(e) = state.db.send_inbox_message(
                        workspace_name,
                        &thread_id,
                        &format!("workflow-run-{}", run.id),
                        agent_name,
                        "request",
                        Some(&serde_json::json!(input)),
                    ) {
                        return Err(e.into());
                    }
                }
                Action::WaitForInput { step } => {
                    if let Err(e) = state.db.set_workflow_step_run_waiting_for_input(&step.id) {
                        return Err(e.into());
                    }
                }
                Action::Complete { step, output } => {
                    let output_ref = if output.is_empty() {
                        None
                    } else {
                        Some(output.as_str())
                    };
                    if let Err(e) = state
                        .db
                        .complete_workflow_step_run(&step.id, "done", output_ref, None)
                    {
                        return Err(e.into());
                    }
                }
            }
        }
    }

    let step_runs = match state.db.list_workflow_step_runs(run_id) {
        Ok(steps) => steps,
        Err(e) => return Err(e.into()),
    };
    let (status, error) = compute_workflow_run_state(&step_runs);
    if let Err(e) =
        state
            .db
            .update_workflow_run_status(workspace_name, run_id, status, error.as_deref())
    {
        return Err(e.into());
    }
    state.inbox_notify.notify_waiters();
    Ok(())
}

async fn handle_workflow_thread_message(
    state: &AppState,
    workspace_name: &str,
    thread_id: &str,
    msg_type: &str,
    content: Option<&serde_json::Value>,
) -> anyhow::Result<()> {
    let Some(step_run) = (match state
        .db
        .get_workflow_step_run_by_thread(workspace_name, thread_id)
    {
        Ok(value) => value,
        Err(e) => return Err(e.into()),
    }) else {
        return Ok(());
    };

    let output_text = json_content_as_text(content);
    match msg_type {
        "started" => {
            if let Err(e) = state.db.mark_workflow_step_run_started(&step_run.id) {
                return Err(e.into());
            }
        }
        "done" => {
            if let Err(e) = state.db.complete_workflow_step_run(
                &step_run.id,
                "done",
                output_text.as_deref(),
                None,
            ) {
                return Err(e.into());
            }
            advance_workflow_run(state, workspace_name, &step_run.workflow_run_id).await?;
        }
        "failed" => {
            if let Err(e) = state.db.complete_workflow_step_run(
                &step_run.id,
                "failed",
                output_text.as_deref(),
                output_text.as_deref(),
            ) {
                return Err(e.into());
            }
            advance_workflow_run(state, workspace_name, &step_run.workflow_run_id).await?;
        }
        "question" => {
            if let Err(e) = state
                .db
                .set_workflow_step_run_waiting_for_input(&step_run.id)
            {
                return Err(e.into());
            }
        }
        _ => {}
    }

    let step_runs = match state.db.list_workflow_step_runs(&step_run.workflow_run_id) {
        Ok(steps) => steps,
        Err(e) => return Err(e.into()),
    };
    let (status, error) = compute_workflow_run_state(&step_runs);
    if let Err(e) = state.db.update_workflow_run_status(
        workspace_name,
        &step_run.workflow_run_id,
        status,
        error.as_deref(),
    ) {
        return Err(e.into());
    }
    Ok(())
}

async fn list_workflows_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state.db.list_workflows(&workspace_name) {
        Ok(workflows) => (
            StatusCode::OK,
            Json(serde_json::json!({"workflows": workflows})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn create_workflow_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
    Json(req): Json<CreateWorkflowRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    if req.name.trim().is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "workflow name is required");
    }
    if !validate_workflow_status(&req.status) {
        return error_response(StatusCode::BAD_REQUEST, "invalid workflow status");
    }

    let (nodes, edges) =
        match validate_workflow_definition(&state, &workspace_name, &req.nodes, &req.edges) {
            Ok(value) => value,
            Err(err) => return err,
        };

    match state.db.create_workflow(
        &workspace_name,
        req.name.trim(),
        req.description.trim(),
        &req.status,
        &caller.user.id,
        &nodes,
        &edges,
    ) {
        Ok(workflow) => (
            StatusCode::CREATED,
            Json(serde_json::to_value(workflow).unwrap()),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn get_workflow_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, workflow_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state
        .db
        .get_workflow_definition(&workspace_name, &workflow_id)
    {
        Ok(Some(definition)) => (
            StatusCode::OK,
            Json(serde_json::to_value(definition).unwrap()),
        )
            .into_response(),
        Ok(None) => error_response(StatusCode::NOT_FOUND, "workflow not found"),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn update_workflow_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, workflow_id)): Path<(String, String)>,
    Json(req): Json<UpdateWorkflowRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    if req.name.trim().is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "workflow name is required");
    }
    if !validate_workflow_status(&req.status) {
        return error_response(StatusCode::BAD_REQUEST, "invalid workflow status");
    }

    let (nodes, edges) =
        match validate_workflow_definition(&state, &workspace_name, &req.nodes, &req.edges) {
            Ok(value) => value,
            Err(err) => return err,
        };
    let user_id = if caller.user.is_admin {
        ""
    } else {
        caller.user.id.as_str()
    };

    match state.db.update_workflow(
        &workspace_name,
        &workflow_id,
        req.name.trim(),
        req.description.trim(),
        &req.status,
        user_id,
        &nodes,
        &edges,
    ) {
        Ok(_) => match state
            .db
            .get_workflow_definition(&workspace_name, &workflow_id)
        {
            Ok(Some(definition)) => (
                StatusCode::OK,
                Json(serde_json::to_value(definition).unwrap()),
            )
                .into_response(),
            Ok(None) => error_response(StatusCode::NOT_FOUND, "workflow not found"),
            Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        },
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn remove_workflow_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, workflow_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    let user_id = if caller.user.is_admin {
        ""
    } else {
        caller.user.id.as_str()
    };
    match state
        .db
        .remove_workflow(&workspace_name, &workflow_id, user_id)
    {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({"status": "removed"})),
        )
            .into_response(),
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn publish_workflow_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, workflow_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    let user_id = if caller.user.is_admin {
        ""
    } else {
        caller.user.id.as_str()
    };
    match state
        .db
        .set_workflow_status(&workspace_name, &workflow_id, "published", user_id)
    {
        Ok(()) => match state.db.get_workflow_definition(&workspace_name, &workflow_id) {
            Ok(Some(definition)) => (
                StatusCode::OK,
                Json(serde_json::to_value(definition).unwrap()),
            )
                .into_response(),
            Ok(None) => error_response(StatusCode::NOT_FOUND, "workflow not found"),
            Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
        },
        Err(e) => error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    }
}

async fn list_workflow_runs_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
    Query(params): Query<WorkflowRunsQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match state
        .db
        .list_workflow_runs(&workspace_name, params.workflow_id.as_deref(), params.limit)
    {
        Ok(runs) => (StatusCode::OK, Json(serde_json::json!({"runs": runs}))).into_response(),
        Err(e) => error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }
}

async fn create_workflow_run_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, workflow_id)): Path<(String, String)>,
    Json(req): Json<CreateWorkflowRunRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }

    match state.db.get_workflow(&workspace_name, &workflow_id) {
        Ok(Some(workflow)) => {
            if workflow.status == "archived" {
                return error_response(StatusCode::BAD_REQUEST, "cannot run an archived workflow");
            }
        }
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "workflow not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    }

    let run = match state.db.create_workflow_run(
        &workspace_name,
        &workflow_id,
        req.input.as_deref(),
        &caller.user.id,
    ) {
        Ok(run) => run,
        Err(e) => return error_response(StatusCode::BAD_REQUEST, &e.to_string()),
    };

    if let Err(err) = advance_workflow_run(&state, &workspace_name, &run.id).await {
        return internal_error(err);
    }
    match workflow_run_response(&state, &workspace_name, &run.id) {
        Ok(payload) => (StatusCode::CREATED, Json(payload)).into_response(),
        Err(err) => err,
    }
}

async fn get_workflow_run_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, run_id)): Path<(String, String)>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    match workflow_run_response(&state, &workspace_name, &run_id) {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(err) => err,
    }
}

async fn retry_workflow_step_run_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, run_id, step_run_id)): Path<(String, String, String)>,
    Query(query): Query<RetryStepQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }

    let run = match state.db.get_workflow_run(&workspace_name, &run_id) {
        Ok(Some(run)) => run,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "workflow run not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let definition = match state
        .db
        .get_workflow_definition(&workspace_name, &run.workflow_id)
    {
        Ok(Some(definition)) => definition,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "workflow not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    let step = match state.db.get_workflow_step_run(&run_id, &step_run_id) {
        Ok(Some(step)) => step,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "workflow step run not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    if step.node_kind == "start" {
        return error_response(StatusCode::BAD_REQUEST, "cannot retry the start node");
    }

    let step_runs = match state.db.list_workflow_step_runs(&run_id) {
        Ok(steps) => steps,
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };

    let reset_ids = if query.scope == "self" {
        // Only reset this step
        vec![step.id.clone()]
    } else {
        // Reset this step and all descendants (default)
        let descendants = descendant_node_ids(&definition, &step.node_id);
        step_runs
            .iter()
            .filter(|candidate| descendants.contains(&candidate.node_id))
            .map(|candidate| candidate.id.clone())
            .collect::<Vec<_>>()
    };
    if let Err(e) = state.db.reset_workflow_step_runs(&run_id, &reset_ids) {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    if let Err(e) = state
        .db
        .update_workflow_run_status(&workspace_name, &run_id, "queued", None)
    {
        return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
    }
    if let Err(err) = advance_workflow_run(&state, &workspace_name, &run_id).await {
        return internal_error(err);
    }
    match workflow_run_response(&state, &workspace_name, &run_id) {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(err) => err,
    }
}

async fn workflow_step_input_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path((workspace_name, run_id, step_run_id)): Path<(String, String, String)>,
    Json(req): Json<WorkflowStepInputRequest>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    if req.input.trim().is_empty() {
        return error_response(StatusCode::BAD_REQUEST, "input is required");
    }

    let step = match state.db.get_workflow_step_run(&run_id, &step_run_id) {
        Ok(Some(step)) => step,
        Ok(None) => return error_response(StatusCode::NOT_FOUND, "workflow step run not found"),
        Err(e) => return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string()),
    };
    if step.status != "waiting_for_input" {
        return error_response(StatusCode::BAD_REQUEST, "step is not waiting for input");
    }

    match step.node_kind.as_str() {
        "human_input" => {
            if let Err(e) = state.db.set_workflow_step_run_output(
                &step.id,
                "done",
                Some(req.input.trim()),
                Some(req.input.trim()),
            ) {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
            if let Err(err) = advance_workflow_run(&state, &workspace_name, &run_id).await {
                return internal_error(err);
            }
        }
        "agent" => {
            let Some(agent_name) = step.agent_name.as_deref() else {
                return error_response(StatusCode::BAD_REQUEST, "agent step is missing an agent");
            };
            let Some(thread_id) = step.thread_id.as_deref() else {
                return error_response(StatusCode::BAD_REQUEST, "agent step is missing a thread");
            };
            if let Err(e) = state.db.set_workflow_step_run_output(
                &step.id,
                "running",
                Some(req.input.trim()),
                None,
            ) {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
            if let Err(e) = state.db.send_inbox_message(
                &workspace_name,
                thread_id,
                &format!("workflow-run-{}", run_id),
                agent_name,
                "answer",
                Some(&serde_json::json!(req.input.trim())),
            ) {
                return error_response(StatusCode::INTERNAL_SERVER_ERROR, &e.to_string());
            }
            state.inbox_notify.notify_waiters();
        }
        _ => {
            return error_response(
                StatusCode::BAD_REQUEST,
                "this step type does not accept input",
            );
        }
    }

    match workflow_run_response(&state, &workspace_name, &run_id) {
        Ok(payload) => (StatusCode::OK, Json(payload)).into_response(),
        Err(err) => err,
    }
}

// --- Threads ---

#[derive(Deserialize)]
struct ThreadsQuery {
    #[serde(default)]
    from_id: Option<String>,
    #[serde(default = "default_threads_limit")]
    limit: i64,
}

fn default_threads_limit() -> i64 {
    20
}

async fn list_threads_handler(
    State(state): State<SharedState>,
    Extension(caller): Extension<Caller>,
    Path(workspace_name): Path<String>,
    Query(params): Query<ThreadsQuery>,
) -> impl IntoResponse {
    if let Err(e) = require_workspace_member(&state, &caller, &workspace_name) {
        return e;
    }
    let from_id = params.from_id.unwrap_or_else(|| caller.user.id.clone());
    match state
        .db
        .list_threads(&workspace_name, &from_id, params.limit)
    {
        Ok(threads) => (
            StatusCode::OK,
            Json(serde_json::json!({"threads": threads})),
        )
            .into_response(),
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
        .route(
            "/workspaces",
            get(list_workspaces_handler).post(create_workspace_handler),
        )
        .route(
            "/workspaces/{workspace_name}/members/{user_id}",
            post(add_to_workspace_handler),
        )
        .route(
            "/workspaces/{workspace_name}/agents/{agent_name}/inbox",
            get(get_inbox_messages_handler).post(send_inbox_message_handler),
        )
        .route(
            "/workspaces/{workspace_name}/inbox/{message_id}/ack",
            post(ack_inbox_message_handler),
        )
        .route(
            "/workspaces/{workspace_name}/threads",
            get(list_threads_handler),
        )
        .route(
            "/workspaces/{workspace_name}/threads/{thread_id}",
            get(get_thread_messages_handler),
        )
        .route(
            "/workspaces/{workspace_name}/agents",
            get(list_agents_handler).post(register_agent_handler),
        )
        .route(
            "/workspaces/{workspace_name}/agents/{name}/threads",
            get(list_agent_threads_handler),
        )
        .route(
            "/workspaces/{workspace_name}/agents/{name}",
            get(get_agent_handler)
                .delete(remove_agent_handler)
                .put(update_agent_handler),
        )
        .route(
            "/workspaces/{workspace_name}/agents/{name}/stop",
            post(stop_agent_handler),
        )
        .route(
            "/workspaces/{workspace_name}/agents/{name}/start",
            post(start_agent_handler),
        )
        .route(
            "/workspaces/{workspace_name}/agents/{name}/logs",
            get(agent_logs_handler),
        )
        .route(
            "/machines",
            get(list_machines_handler).post(register_machine_handler),
        )
        .route(
            "/machines/{machine_id}/heartbeat",
            post(heartbeat_machine_handler),
        )
        .route("/machines/{machine_id}/agents", get(machine_agents_handler))
        .route("/machines/{machine_id}/poll", get(machine_poll_handler))
        .route("/users", get(list_users_handler))
        .route("/users/invite", post(invite_user_handler))
        .route(
            "/workspaces/{workspace_name}/workflows",
            get(list_workflows_handler).post(create_workflow_handler),
        )
        .route(
            "/workspaces/{workspace_name}/workflows/{workflow_id}",
            get(get_workflow_handler)
                .put(update_workflow_handler)
                .delete(remove_workflow_handler),
        )
        .route(
            "/workspaces/{workspace_name}/workflows/{workflow_id}/publish",
            post(publish_workflow_handler),
        )
        .route(
            "/workspaces/{workspace_name}/workflows/{workflow_id}/runs",
            post(create_workflow_run_handler),
        )
        .route(
            "/workspaces/{workspace_name}/workflow-runs",
            get(list_workflow_runs_handler),
        )
        .route(
            "/workspaces/{workspace_name}/workflow-runs/{run_id}",
            get(get_workflow_run_handler),
        )
        .route(
            "/workspaces/{workspace_name}/workflow-runs/{run_id}/steps/{step_run_id}/retry",
            post(retry_workflow_step_run_handler),
        )
        .route(
            "/workspaces/{workspace_name}/workflow-runs/{run_id}/steps/{step_run_id}/input",
            post(workflow_step_input_handler),
        )
        // Tasks (workspace-scoped)
        .route(
            "/workspaces/{workspace_name}/tasks",
            get(list_tasks_handler).post(create_task_handler),
        )
        .route(
            "/workspaces/{workspace_name}/tasks/{task_id}",
            get(get_task_handler),
        )
        .route(
            "/workspaces/{workspace_name}/tasks/{task_id}/messages",
            post(send_task_message_handler),
        )
        // Cron jobs (workspace-scoped)
        .route(
            "/workspaces/{workspace_name}/cron",
            get(list_cron_handler).post(create_cron_handler),
        )
        .route(
            "/workspaces/{workspace_name}/cron/{cron_id}",
            delete(remove_cron_handler).put(update_cron_handler),
        )
        .layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    let (web_dir, index_file) = frontend_assets_dir();
    let serve_dir = ServeDir::new(&web_dir).fallback(ServeFile::new(index_file));

    Router::new()
        .merge(public)
        .merge(protected)
        .fallback_service(serve_dir)
        .layer(CorsLayer::permissive())
        .with_state(state)
}

fn frontend_assets_dir() -> (std::path::PathBuf, std::path::PathBuf) {
    let frontend_dist = std::path::PathBuf::from("frontend/dist");
    let frontend_index = frontend_dist.join("index.html");
    if frontend_index.exists() {
        return (frontend_dist, frontend_index);
    }

    let legacy_web = std::path::PathBuf::from("web");
    let legacy_index = legacy_web.join("index.html");
    (legacy_web, legacy_index)
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
    agents_path: &str,
    api_key: Option<&str>,
    first_start: Option<(&str, &str, &str)>, // (key, user_name, user_id)
) {
    let version = env!("CARGO_PKG_VERSION");
    let mut lines = Vec::new();

    lines.push(banner_top());
    // Logo
    for logo_line in LOGO.lines() {
        if logo_line.is_empty() {
            continue;
        }
        lines.push(banner_line(logo_line));
    }
    lines.push(banner_empty());
    lines.push(banner_line(&format!("   v{:<28}{}", version, server_url)));
    lines.push(banner_empty());

    // First start section
    if let Some((key, user_name, user_id)) = first_start {
        lines.push(banner_separator());
        lines.push(banner_empty());
        lines.push(banner_line("   First start detected."));
        lines.push(banner_empty());
        lines.push(banner_line(&format!("   Admin key:  {}", key)));
        lines.push(banner_line(&format!(
            "   User:       {} ({})",
            user_name, user_id
        )));
        lines.push(banner_empty());
        lines.push(banner_line(
            "   CLI auto-configured. No login needed on this machine.",
        ));
        lines.push(banner_line(
            "   Next step:  b0 agent add <name> --instructions \"...\"",
        ));
        lines.push(banner_empty());
    }

    // Get started section (always shown)
    lines.push(banner_separator());
    lines.push(banner_empty());
    lines.push(banner_line("   Get started:"));
    lines.push(banner_empty());
    lines.push(banner_line("   1. b0 skill install claude-code"));
    lines.push(banner_line("      or: b0 skill install codex"));
    lines.push(banner_empty());
    lines.push(banner_line(
        "   2. b0 agent add <name> --instructions \"...\"",
    ));
    lines.push(banner_empty());
    lines.push(banner_line("   3. Open Claude Code and start delegating."));
    lines.push(banner_empty());

    // Info section
    lines.push(banner_separator());
    lines.push(banner_empty());
    lines.push(banner_line(&format!("   Database:   {}", db_path)));
    lines.push(banner_line(&format!("   Agents:     {}", agents_path)));
    lines.push(banner_empty());
    lines.push(banner_line("   Press Ctrl+C to stop."));
    lines.push(banner_empty());
    lines.push(banner_bottom());

    println!();
    for line in &lines {
        println!("{}", line);
    }

    // Dashboard link outside the box (can be long with key)
    let dashboard_url = match api_key {
        Some(key) => format!("{}?key={}", server_url, key),
        None => server_url.to_string(),
    };
    println!("  Dashboard: {}", dashboard_url);
    println!();
}

// --- Server ---

pub async fn run(config: ServerConfig, no_local: bool) {
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
        .join("agents");
    let agents_display = format!("{}/", workspace_root.display());

    // Shorten paths for display: replace home dir with ~
    let db_display = match dirs::home_dir() {
        Some(home) => config
            .db_path
            .replace(&home.to_string_lossy().to_string(), "~"),
        None => config.db_path.clone(),
    };
    let agents_display = match dirs::home_dir() {
        Some(home) => agents_display.replace(&home.to_string_lossy().to_string(), "~"),
        None => agents_display,
    };

    // Bootstrap admin user on first start + auto-configure local CLI
    let first_start_info = match db.bootstrap_admin() {
        Ok(Some((user, key))) => {
            let mut cli_cfg = crate::config::CliConfig::load();
            cli_cfg.server_url = format!("http://127.0.0.1:{}", config.port);
            cli_cfg.api_key = Some(key.clone());
            cli_cfg.default_workspace = Some(user.name.clone());
            let _ = cli_cfg.lead_id();
            if let Err(e) = cli_cfg.save() {
                tracing::warn!("Failed to auto-configure CLI: {}", e);
            }
            Some((key, user.name.clone(), user.id.clone()))
        }
        Ok(None) => None,
        Err(e) => {
            tracing::error!("Failed to bootstrap admin: {}", e);
            None
        }
    };

    // Auto-register "local" machine owned by admin
    if !no_local {
        if let Ok(Some(admin_id)) = db.get_admin_user_id() {
            let _ = db.register_machine("local", &admin_id);
        }
    }

    // Resolve API key for dashboard URL: from first start or CLI config
    let api_key = first_start_info
        .as_ref()
        .map(|(k, _, _)| k.clone())
        .or_else(|| crate::config::CliConfig::load().api_key);

    // Print banner
    print_banner(
        &server_url,
        &db_display,
        &agents_display,
        api_key.as_deref(),
        first_start_info
            .as_ref()
            .map(|(k, n, i)| (k.as_str(), n.as_str(), i.as_str())),
    );

    let state = Arc::new(AppState { db, inbox_notify: tokio::sync::Notify::new(), slack_token: config.slack_token.clone() });

    // Spawn daemon for "local" machine
    if !no_local {
        let daemon_state = state.clone();
        tokio::spawn(async move {
            daemon::run_local(daemon_state, workspace_root).await;
        });
    }

    // Spawn scheduler for cron jobs
    let scheduler_state = state.clone();
    tokio::spawn(async move {
        crate::scheduler::run(scheduler_state).await;
    });

    let app = build_router(state);

    let addr = config.address();

    let listener = match tokio::net::TcpListener::bind(&addr).await {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error: cannot bind to {}. {}", addr, e);
            if e.kind() == std::io::ErrorKind::AddrInUse {
                let port = addr.split(':').last().unwrap_or("8080");
                eprintln!(
                    "Hint: kill the existing process: kill $(lsof -ti :{})",
                    port
                );
                eprintln!("  or use a different port:       b0 server --port <other>",);
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
