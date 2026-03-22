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

    // Register local node
    db.register_node("local", &admin_user.id).unwrap();

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
    let result = client.list_groups().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_auth_rejects_no_key() {
    let (url, _key, _tmp) = start_test_server().await;
    let client = BhClient::new(&url);
    let result = client.list_groups().await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_admin_has_personal_group() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);
    let groups = client.list_groups().await.unwrap();
    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].name, "admin");
}

#[tokio::test]
async fn test_invite_user_and_group_membership() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Invite alice
    let alice = client.invite_user("alice").await.unwrap();
    assert_eq!(alice.name, "alice");
    assert!(alice.key.starts_with("b0_"));

    // Alice has her own personal group
    let alice_client = BhClient::with_api_key(&url, &alice.key);
    let alice_groups = alice_client.list_groups().await.unwrap();
    assert_eq!(alice_groups.len(), 1);
    assert_eq!(alice_groups[0].name, "alice");

    // Admin creates shared group and adds alice
    client.create_group("dev-team").await.unwrap();
    client.add_group_member("dev-team", &alice.user_id).await.unwrap();

    // Alice now sees 2 groups
    let alice_groups = alice_client.list_groups().await.unwrap();
    assert_eq!(alice_groups.len(), 2);
}

#[tokio::test]
async fn test_worker_crud() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Create
    let w = client
        .register_worker("admin", "reviewer", "Code reviewer", "Review code.", "local", "auto")
        .await
        .unwrap();
    assert_eq!(w.name, "reviewer");
    assert_eq!(w.runtime, "auto");

    // List
    let workers = client.list_workers("admin").await.unwrap();
    assert_eq!(workers.len(), 1);

    // Get
    let w = client.get_worker("admin", "reviewer").await.unwrap();
    assert_eq!(w.instructions, "Review code.");

    // Update
    client.update_worker("admin", "reviewer", "Review carefully.").await.unwrap();
    let w = client.get_worker("admin", "reviewer").await.unwrap();
    assert_eq!(w.instructions, "Review carefully.");

    // Stop / start
    client.stop_worker("admin", "reviewer").await.unwrap();
    let w = client.get_worker("admin", "reviewer").await.unwrap();
    assert_eq!(w.status, "stopped");

    client.start_worker("admin", "reviewer").await.unwrap();
    let w = client.get_worker("admin", "reviewer").await.unwrap();
    assert_eq!(w.status, "active");

    // Remove
    client.remove_worker("admin", "reviewer").await.unwrap();
    let workers = client.list_workers("admin").await.unwrap();
    assert_eq!(workers.len(), 0);
}

#[tokio::test]
async fn test_worker_group_isolation() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Invite alice and bob (non-admin users)
    let alice = client.invite_user("alice").await.unwrap();
    let bob = client.invite_user("bob").await.unwrap();
    let alice_client = BhClient::with_api_key(&url, &alice.key);
    let bob_client = BhClient::with_api_key(&url, &bob.key);

    // Alice creates a worker in her personal group
    alice_client
        .register_worker("alice", "alice-worker", "", "Do stuff.", "local", "auto")
        .await
        .unwrap();

    // Bob cannot see alice's workers (not a member of alice's group)
    let result = bob_client.list_workers("alice").await;
    assert!(result.is_err());

    // Bob creates his own worker
    bob_client
        .register_worker("bob", "bob-worker", "", "Do stuff.", "local", "auto")
        .await
        .unwrap();

    // Alice cannot see bob's workers
    let result = alice_client.list_workers("bob").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_worker_ownership_permission() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Create shared group, invite alice
    let alice = client.invite_user("alice").await.unwrap();
    client.create_group("team").await.unwrap();
    client.add_group_member("team", &alice.user_id).await.unwrap();

    let alice_client = BhClient::with_api_key(&url, &alice.key);

    // Alice creates a worker in the shared group
    alice_client
        .register_worker("team", "alice-worker", "", "Do stuff.", "local", "auto")
        .await
        .unwrap();

    // Admin cannot remove alice's worker
    let result = client.remove_worker("team", "alice-worker").await;
    assert!(result.is_err());

    // Alice can remove her own worker
    alice_client.remove_worker("team", "alice-worker").await.unwrap();
}

#[tokio::test]
async fn test_inbox_roundtrip() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Register agents
    client.register_agent("admin", "sender").await.unwrap();
    client.register_agent("admin", "receiver").await.unwrap();

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
async fn test_nodes() {
    let (url, key, _tmp) = start_test_server().await;
    let client = admin_client(&url, &key);

    // Local node exists from bootstrap
    let nodes = client.list_nodes().await.unwrap();
    assert_eq!(nodes.len(), 1);
    assert_eq!(nodes[0].id, "local");

    // Register another node
    client.register_node("gpu-box").await.unwrap();
    let nodes = client.list_nodes().await.unwrap();
    assert_eq!(nodes.len(), 2);

    // Heartbeat
    client.heartbeat_node("gpu-box").await.unwrap();
}
