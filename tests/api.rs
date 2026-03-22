use box0::client::BhClient;
use box0::db::Database;
use box0::server::{AppState, SharedState};
use std::sync::Arc;
use tempfile::TempDir;

/// Start a test server on a random port and return (base_url, admin_key, temp_dir).
/// The temp_dir must be kept alive for the duration of the test.
async fn start_test_server() -> (String, String, TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let db_path = tmp.path().join("test.db");
    let db = Database::new(db_path.to_str().unwrap()).unwrap();

    // Bootstrap admin
    let (admin_user, admin_key) = db.bootstrap_admin().unwrap().unwrap();

    // Register local machine
    db.register_machine("local", &admin_user.id).unwrap();

    let state: SharedState = Arc::new(AppState { db });
    let app = box0::server::build_router(state);

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    let base_url = format!("http://127.0.0.1:{}", addr.port());

    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (base_url, admin_key, tmp)
}

fn admin_client(base_url: &str, key: &str) -> BhClient {
    BhClient::with_api_key(base_url, key)
}

// --- Tests ---

#[tokio::test]
async fn test_health() {
    let (url, _key, _tmp) = start_test_server().await;
    let client = BhClient::new(&url);
    let version = client.health().await.unwrap();
    assert_eq!(version, "0.1.0");
}

#[tokio::test]
async fn test_auth_rejects_bad_key() {
    let (url, _key, _tmp) = start_test_server().await;
    let client = BhClient::with_api_key(&url, "b0_invalid");
    let result = client.list_workspaces().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_auth_rejects_no_key() {
    let (url, _key, _tmp) = start_test_server().await;
    let client = BhClient::new(&url);
    let result = client.list_workspaces().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_admin_has_personal_workspace() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);
    let workspaces = client.list_workspaces().await.unwrap();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].name, "admin");
}

#[tokio::test]
async fn test_invite_user_and_workspace_membership() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Invite alice
    let alice = client.invite_user("alice").await.unwrap();
    assert_eq!(alice.name, "alice");
    assert!(alice.key.starts_with("b0_"));

    // Alice has her own personal workspace
    let alice_client = BhClient::with_api_key(&url, &alice.key);
    let alice_workspaces = alice_client.list_workspaces().await.unwrap();
    assert_eq!(alice_workspaces.len(), 1);
    assert_eq!(alice_workspaces[0].name, "alice");

    // Admin creates shared workspace and adds alice
    client.create_workspace("dev-team").await.unwrap();
    client.add_workspace_member("dev-team", &alice.user_id).await.unwrap();

    // Alice now sees 2 workspaces
    let alice_workspaces = alice_client.list_workspaces().await.unwrap();
    assert_eq!(alice_workspaces.len(), 2);
}

#[tokio::test]
async fn test_agent_crud() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Create
    let a = client
        .register_agent("admin", "reviewer", "Code reviewer", "Review code.", "local", "auto")
        .await
        .unwrap();
    assert_eq!(a.name, "reviewer");
    assert_eq!(a.runtime, "auto");

    // List
    let agents = client.list_agents("admin").await.unwrap();
    assert_eq!(agents.len(), 1);

    // Get
    let a = client.get_agent("admin", "reviewer").await.unwrap();
    assert_eq!(a.instructions, "Review code.");

    // Update
    client.update_agent("admin", "reviewer", "Review carefully.").await.unwrap();
    let a = client.get_agent("admin", "reviewer").await.unwrap();
    assert_eq!(a.instructions, "Review carefully.");

    // Stop / start
    client.stop_agent("admin", "reviewer").await.unwrap();
    let a = client.get_agent("admin", "reviewer").await.unwrap();
    assert_eq!(a.status, "stopped");

    client.start_agent("admin", "reviewer").await.unwrap();
    let a = client.get_agent("admin", "reviewer").await.unwrap();
    assert_eq!(a.status, "active");

    // Remove
    client.remove_agent("admin", "reviewer").await.unwrap();
    let agents = client.list_agents("admin").await.unwrap();
    assert_eq!(agents.len(), 0);
}

#[tokio::test]
async fn test_agent_workspace_isolation() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Invite alice and bob (non-admin users)
    let alice = client.invite_user("alice").await.unwrap();
    let bob = client.invite_user("bob").await.unwrap();
    let alice_client = BhClient::with_api_key(&url, &alice.key);
    let bob_client = BhClient::with_api_key(&url, &bob.key);

    // Alice creates an agent in her personal workspace
    alice_client
        .register_agent("alice", "alice-agent", "", "Do stuff.", "local", "auto")
        .await
        .unwrap();

    // Bob cannot see alice's agents (not a member of alice's workspace)
    let result = bob_client.list_agents("alice").await;
    assert!(result.is_err());

    // Bob creates his own agent
    bob_client
        .register_agent("bob", "bob-agent", "", "Do stuff.", "local", "auto")
        .await
        .unwrap();

    // Alice cannot see bob's agents
    let result = alice_client.list_agents("bob").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_agent_ownership_permission() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Create shared workspace, invite alice
    let alice = client.invite_user("alice").await.unwrap();
    client.create_workspace("team").await.unwrap();
    client.add_workspace_member("team", &alice.user_id).await.unwrap();

    let alice_client = BhClient::with_api_key(&url, &alice.key);

    // Alice creates an agent in the shared workspace
    alice_client
        .register_agent("team", "alice-agent", "", "Do stuff.", "local", "auto")
        .await
        .unwrap();

    // Admin cannot remove alice's agent
    let result = client.remove_agent("team", "alice-agent").await;
    assert!(result.is_err());

    // Alice can remove her own agent
    alice_client.remove_agent("team", "alice-agent").await.unwrap();
}

#[tokio::test]
async fn test_inbox_roundtrip() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Register agents
    client.register_agent("admin", "sender", "", "Send stuff.", "local", "auto").await.unwrap();
    client.register_agent("admin", "receiver", "", "Receive stuff.", "local", "auto").await.unwrap();

    // Send message
    let content = serde_json::json!("hello");
    client
        .send_message("admin", "receiver", "thread-1", "sender", "request", Some(&content))
        .await
        .unwrap();

    // Read inbox
    let messages = client.get_inbox("admin", "receiver", Some("unread"), None).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].msg_type, "request");
    assert_eq!(messages[0].thread_id, "thread-1");

    // Ack
    client.ack_message("admin", &messages[0].id).await.unwrap();

    // Inbox empty after ack
    let messages = client.get_inbox("admin", "receiver", Some("unread"), None).await.unwrap();
    assert_eq!(messages.len(), 0);
}

#[tokio::test]
async fn test_started_message_flow() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Register lead and worker agent
    client.register_agent("admin", "lead", "", "Lead agent.", "local", "auto").await.unwrap();
    client.register_agent("admin", "worker-1", "", "Worker agent.", "local", "auto").await.unwrap();

    // Simulate: lead sends request to worker
    client
        .send_message("admin", "worker-1", "thread-1", "lead", "request", Some(&serde_json::json!("task")))
        .await
        .unwrap();

    // Simulate: daemon sends "started" back to lead
    client
        .send_message("admin", "lead", "thread-1", "worker-1", "started", None)
        .await
        .unwrap();

    // Lead sees the started message
    let messages = client.get_inbox("admin", "lead", Some("unread"), None).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].msg_type, "started");
    assert_eq!(messages[0].from_id, "worker-1");

    // Ack it
    client.ack_message("admin", &messages[0].id).await.unwrap();

    // Simulate: daemon sends "done" back to lead
    client
        .send_message("admin", "lead", "thread-1", "worker-1", "done", Some(&serde_json::json!("result")))
        .await
        .unwrap();

    let messages = client.get_inbox("admin", "lead", Some("unread"), None).await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].msg_type, "done");
}

#[tokio::test]
async fn test_machines() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Local machine exists from bootstrap
    let machines = client.list_machines().await.unwrap();
    assert_eq!(machines.len(), 1);
    assert_eq!(machines[0].id, "local");

    // Register another machine
    client.register_machine("gpu-box").await.unwrap();
    let machines = client.list_machines().await.unwrap();
    assert_eq!(machines.len(), 2);

    // Heartbeat
    client.heartbeat_machine("gpu-box").await.unwrap();
}

#[tokio::test]
async fn test_cron_crud() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Create an agent first
    client
        .register_agent("admin", "seo-agent", "SEO checker", "Check SEO.", "local", "auto")
        .await
        .unwrap();

    // Create cron job
    let job = client.create_cron_job("admin", "seo-agent", "6h", "Check the website SEO").await.unwrap();
    assert!(job.id.starts_with("cron-"));
    assert_eq!(job.agent, "seo-agent");
    assert_eq!(job.schedule, "6h");
    assert!(job.enabled);

    // List
    let jobs = client.list_cron_jobs("admin").await.unwrap();
    assert_eq!(jobs.len(), 1);
    assert_eq!(jobs[0].id, job.id);

    // Disable
    client.set_cron_enabled("admin", &job.id, false).await.unwrap();
    let jobs = client.list_cron_jobs("admin").await.unwrap();
    assert!(!jobs[0].enabled);

    // Enable
    client.set_cron_enabled("admin", &job.id, true).await.unwrap();
    let jobs = client.list_cron_jobs("admin").await.unwrap();
    assert!(jobs[0].enabled);

    // Remove
    client.remove_cron_job("admin", &job.id).await.unwrap();
    let jobs = client.list_cron_jobs("admin").await.unwrap();
    assert_eq!(jobs.len(), 0);
}

#[tokio::test]
async fn test_cron_invalid_schedule() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    client
        .register_agent("admin", "agent", "", "Do stuff.", "local", "auto")
        .await
        .unwrap();

    // Invalid schedule should fail
    let result = client.create_cron_job("admin", "agent", "invalid", "task").await;
    assert!(result.is_err());
}
