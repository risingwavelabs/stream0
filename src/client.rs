use anyhow::Result;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Clone)]
pub struct BhClient {
    base_url: String,
    client: reqwest::Client,
    api_key: Arc<RwLock<Option<String>>>,
}

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: String,
}

#[derive(Debug, Deserialize)]
struct WorkersResponse {
    workers: Vec<crate::db::Worker>,
}

#[derive(Debug, Deserialize)]
struct InboxResponse {
    messages: Vec<crate::db::InboxMessage>,
}

#[derive(Debug, Deserialize)]
pub struct InviteResponse {
    pub key: String,
    pub user_id: String,
    pub name: String,
}

impl BhClient {
    pub fn new(base_url: &str) -> Self {
        BhClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            api_key: Arc::new(RwLock::new(None)),
        }
    }

    pub fn with_api_key(base_url: &str, api_key: &str) -> Self {
        BhClient {
            base_url: base_url.trim_end_matches('/').to_string(),
            client: reqwest::Client::new(),
            api_key: Arc::new(RwLock::new(Some(api_key.to_string()))),
        }
    }

    pub async fn set_api_key(&self, key: &str) {
        *self.api_key.write().await = Some(key.to_string());
    }

    async fn request(&self, req: reqwest::RequestBuilder) -> Result<reqwest::Response> {
        let req = if let Some(key) = self.api_key.read().await.as_ref() {
            req.header("x-api-key", key)
        } else {
            req
        };
        let resp = req.send().await?;
        self.check_error(resp).await
    }

    async fn check_error(&self, resp: reqwest::Response) -> Result<reqwest::Response> {
        if resp.status().is_success() {
            return Ok(resp);
        }
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        if let Ok(err) = serde_json::from_str::<ErrorResponse>(&body) {
            anyhow::bail!("{}", err.error);
        }
        anyhow::bail!("HTTP {}: {}", status, body);
    }

    // --- Health ---

    pub async fn health(&self) -> Result<String> {
        let resp = self.client.get(format!("{}/health", self.base_url)).send().await?;
        let resp = self.check_error(resp).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body["version"].as_str().unwrap_or("unknown").to_string())
    }

    // --- Agents (group-scoped) ---

    pub async fn register_agent(&self, group: &str, id: &str) -> Result<crate::db::Agent> {
        let req = self.client
            .post(format!("{}/groups/{}/agents", self.base_url, group))
            .json(&serde_json::json!({"id": id}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    // --- Workers (group-scoped) ---

    pub async fn register_worker(&self, group: &str, name: &str, description: &str, instructions: &str, node_id: &str, runtime: &str) -> Result<crate::db::Worker> {
        let req = self.client
            .post(format!("{}/groups/{}/workers", self.base_url, group))
            .json(&serde_json::json!({"name": name, "description": description, "instructions": instructions, "node_id": node_id, "runtime": runtime}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_workers(&self, group: &str) -> Result<Vec<crate::db::Worker>> {
        let req = self.client.get(format!("{}/groups/{}/workers", self.base_url, group));
        let resp = self.request(req).await?;
        let body: WorkersResponse = resp.json().await?;
        Ok(body.workers)
    }

    pub async fn get_worker(&self, group: &str, name: &str) -> Result<crate::db::Worker> {
        let req = self.client.get(format!("{}/groups/{}/workers/{}", self.base_url, group, name));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn remove_worker(&self, group: &str, name: &str) -> Result<()> {
        let req = self.client.delete(format!("{}/groups/{}/workers/{}", self.base_url, group, name));
        self.request(req).await?;
        Ok(())
    }

    pub async fn update_worker(&self, group: &str, name: &str, instructions: &str) -> Result<()> {
        let req = self.client
            .put(format!("{}/groups/{}/workers/{}", self.base_url, group, name))
            .json(&serde_json::json!({"instructions": instructions}));
        self.request(req).await?;
        Ok(())
    }

    pub async fn stop_worker(&self, group: &str, name: &str) -> Result<()> {
        let req = self.client.post(format!("{}/groups/{}/workers/{}/stop", self.base_url, group, name));
        self.request(req).await?;
        Ok(())
    }

    pub async fn start_worker(&self, group: &str, name: &str) -> Result<()> {
        let req = self.client.post(format!("{}/groups/{}/workers/{}/start", self.base_url, group, name));
        self.request(req).await?;
        Ok(())
    }

    pub async fn worker_logs(&self, group: &str, name: &str) -> Result<Vec<crate::db::InboxMessage>> {
        let req = self.client.get(format!("{}/groups/{}/workers/{}/logs", self.base_url, group, name));
        let resp = self.request(req).await?;
        let body: InboxResponse = resp.json().await?;
        Ok(body.messages)
    }

    // --- Inbox (group-scoped) ---

    pub async fn send_message(&self, group: &str, to: &str, thread_id: &str, from: &str, msg_type: &str, content: Option<&serde_json::Value>) -> Result<()> {
        let mut body = serde_json::json!({"thread_id": thread_id, "from": from, "type": msg_type});
        if let Some(c) = content {
            body["content"] = c.clone();
        }
        let req = self.client
            .post(format!("{}/groups/{}/agents/{}/inbox", self.base_url, group, to))
            .json(&body);
        self.request(req).await?;
        Ok(())
    }

    pub async fn get_inbox(&self, group: &str, agent_id: &str, status: Option<&str>, timeout: Option<f64>) -> Result<Vec<crate::db::InboxMessage>> {
        let mut url = format!("{}/groups/{}/agents/{}/inbox", self.base_url, group, agent_id);
        let mut params = vec![];
        if let Some(s) = status { params.push(format!("status={}", s)); }
        if let Some(t) = timeout { params.push(format!("timeout={}", t)); }
        if !params.is_empty() { url = format!("{}?{}", url, params.join("&")); }

        let req = self.client.get(&url);
        let resp = self.request(req).await?;
        let body: InboxResponse = resp.json().await?;
        Ok(body.messages)
    }

    pub async fn ack_message(&self, group: &str, message_id: &str) -> Result<()> {
        let req = self.client.post(format!("{}/groups/{}/inbox/{}/ack", self.base_url, group, message_id));
        self.request(req).await?;
        Ok(())
    }

    // --- Nodes (global) ---

    pub async fn register_node(&self, id: &str) -> Result<crate::db::Node> {
        let req = self.client
            .post(format!("{}/nodes", self.base_url))
            .json(&serde_json::json!({"id": id}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_nodes(&self) -> Result<Vec<crate::db::Node>> {
        let req = self.client.get(format!("{}/nodes", self.base_url));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(body["nodes"].clone()).unwrap_or_default())
    }

    /// Get all active workers on a node across all groups (for daemon use).
    pub async fn node_workers(&self, node_id: &str) -> Result<Vec<(String, crate::db::Worker)>> {
        let req = self.client.get(format!("{}/nodes/{}/workers", self.base_url, node_id));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        let items = body["workers"].as_array().cloned().unwrap_or_default();
        let mut result = vec![];
        for item in items {
            let group = item["group"].as_str().unwrap_or("").to_string();
            if let Ok(w) = serde_json::from_value::<crate::db::Worker>(item.clone()) {
                result.push((group, w));
            }
        }
        Ok(result)
    }

    pub async fn heartbeat_node(&self, id: &str) -> Result<()> {
        let req = self.client.post(format!("{}/nodes/{}/heartbeat", self.base_url, id));
        self.request(req).await?;
        Ok(())
    }

    // --- Groups ---

    pub async fn create_group(&self, name: &str) -> Result<crate::db::Group> {
        let req = self.client
            .post(format!("{}/groups", self.base_url))
            .json(&serde_json::json!({"name": name}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_groups(&self) -> Result<Vec<crate::db::Group>> {
        let req = self.client.get(format!("{}/groups", self.base_url));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(body["groups"].clone()).unwrap_or_default())
    }

    pub async fn add_group_member(&self, group: &str, user_id: &str) -> Result<()> {
        let req = self.client.post(format!("{}/groups/{}/members/{}", self.base_url, group, user_id));
        self.request(req).await?;
        Ok(())
    }

    // --- Users ---

    pub async fn invite_user(&self, name: &str) -> Result<InviteResponse> {
        let req = self.client
            .post(format!("{}/users/invite", self.base_url))
            .json(&serde_json::json!({"name": name}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }
}
