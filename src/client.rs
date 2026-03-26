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
struct AgentsResponse {
    agents: Vec<crate::db::Agent>,
}

#[derive(Debug, Deserialize)]
struct InboxResponse {
    messages: Vec<crate::db::InboxMessage>,
}

#[derive(Debug, Deserialize)]
struct MachineInboxResponse {
    messages: Vec<crate::db::MachineInboxMessage>,
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
        let resp = self
            .client
            .get(format!("{}/health", self.base_url))
            .send()
            .await?;
        let resp = self.check_error(resp).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(body["version"].as_str().unwrap_or("unknown").to_string())
    }

    // --- Agents (workspace-scoped) ---

    pub async fn register_agent(
        &self,
        workspace: &str,
        name: &str,
        description: &str,
        instructions: &str,
        machine_id: &str,
        runtime: &str,
        kind: &str,
        webhook_url: Option<&str>,
        slack_channel: Option<&str>,
    ) -> Result<crate::db::Agent> {
        let mut body = serde_json::json!({"name": name, "description": description, "instructions": instructions, "machine_id": machine_id, "runtime": runtime, "kind": kind});
        if let Some(url) = webhook_url { body["webhook_url"] = serde_json::json!(url); }
        if let Some(ch) = slack_channel { body["slack_channel"] = serde_json::json!(ch); }
        let req = self.client
            .post(format!("{}/workspaces/{}/agents", self.base_url, workspace))
            .json(&body);
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_agents(&self, workspace: &str) -> Result<Vec<crate::db::Agent>> {
        let req = self
            .client
            .get(format!("{}/workspaces/{}/agents", self.base_url, workspace));
        let resp = self.request(req).await?;
        let body: AgentsResponse = resp.json().await?;
        Ok(body.agents)
    }

    pub async fn get_agent(&self, workspace: &str, name: &str) -> Result<crate::db::Agent> {
        let req = self.client.get(format!(
            "{}/workspaces/{}/agents/{}",
            self.base_url, workspace, name
        ));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn remove_agent(&self, workspace: &str, name: &str) -> Result<()> {
        let req = self.client.delete(format!(
            "{}/workspaces/{}/agents/{}",
            self.base_url, workspace, name
        ));
        self.request(req).await?;
        Ok(())
    }

    pub async fn update_agent(
        &self,
        workspace: &str,
        name: &str,
        instructions: &str,
    ) -> Result<()> {
        let req = self
            .client
            .put(format!(
                "{}/workspaces/{}/agents/{}",
                self.base_url, workspace, name
            ))
            .json(&serde_json::json!({"instructions": instructions}));
        self.request(req).await?;
        Ok(())
    }

    pub async fn stop_agent(&self, workspace: &str, name: &str) -> Result<()> {
        let req = self.client.post(format!(
            "{}/workspaces/{}/agents/{}/stop",
            self.base_url, workspace, name
        ));
        self.request(req).await?;
        Ok(())
    }

    pub async fn start_agent(&self, workspace: &str, name: &str) -> Result<()> {
        let req = self.client.post(format!(
            "{}/workspaces/{}/agents/{}/start",
            self.base_url, workspace, name
        ));
        self.request(req).await?;
        Ok(())
    }

    pub async fn agent_logs(
        &self,
        workspace: &str,
        name: &str,
    ) -> Result<Vec<crate::db::InboxMessage>> {
        let req = self.client.get(format!(
            "{}/workspaces/{}/agents/{}/logs",
            self.base_url, workspace, name
        ));
        let resp = self.request(req).await?;
        let body: InboxResponse = resp.json().await?;
        Ok(body.messages)
    }

    // --- Inbox (workspace-scoped) ---

    pub async fn send_message(
        &self,
        workspace: &str,
        to: &str,
        thread_id: &str,
        from: &str,
        msg_type: &str,
        content: Option<&serde_json::Value>,
    ) -> Result<()> {
        let mut body = serde_json::json!({"thread_id": thread_id, "from": from, "type": msg_type});
        if let Some(c) = content {
            body["content"] = c.clone();
        }
        let req = self
            .client
            .post(format!(
                "{}/workspaces/{}/agents/{}/inbox",
                self.base_url, workspace, to
            ))
            .json(&body);
        self.request(req).await?;
        Ok(())
    }

    pub async fn get_inbox(
        &self,
        workspace: &str,
        agent_name: &str,
        status: Option<&str>,
        timeout: Option<f64>,
    ) -> Result<Vec<crate::db::InboxMessage>> {
        let mut url = format!(
            "{}/workspaces/{}/agents/{}/inbox",
            self.base_url, workspace, agent_name
        );
        let mut params = vec![];
        if let Some(s) = status {
            params.push(format!("status={}", s));
        }
        if let Some(t) = timeout {
            params.push(format!("timeout={}", t));
        }
        if !params.is_empty() {
            url = format!("{}?{}", url, params.join("&"));
        }

        let req = self.client.get(&url);
        let resp = self.request(req).await?;
        let body: InboxResponse = resp.json().await?;
        Ok(body.messages)
    }

    pub async fn ack_message(&self, workspace: &str, message_id: &str) -> Result<()> {
        let req = self.client.post(format!(
            "{}/workspaces/{}/inbox/{}/ack",
            self.base_url, workspace, message_id
        ));
        self.request(req).await?;
        Ok(())
    }

    // --- Machines (global) ---

    pub async fn register_machine(&self, id: &str) -> Result<crate::db::Machine> {
        let req = self
            .client
            .post(format!("{}/machines", self.base_url))
            .json(&serde_json::json!({"id": id}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_machines(&self) -> Result<Vec<crate::db::Machine>> {
        let req = self.client.get(format!("{}/machines", self.base_url));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(body["machines"].clone()).unwrap_or_default())
    }

    /// Get all active agents on a machine across all workspaces (for daemon use).
    pub async fn machine_agents(
        &self,
        machine_id: &str,
    ) -> Result<Vec<(String, crate::db::Agent)>> {
        let req = self
            .client
            .get(format!("{}/machines/{}/agents", self.base_url, machine_id));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        let items = body["agents"].as_array().cloned().unwrap_or_default();
        let mut result = vec![];
        for item in items {
            let workspace = item["workspace"].as_str().unwrap_or("").to_string();
            if let Ok(a) = serde_json::from_value::<crate::db::Agent>(item.clone()) {
                result.push((workspace, a));
            }
        }
        Ok(result)
    }

    pub async fn heartbeat_machine(&self, id: &str) -> Result<()> {
        let req = self
            .client
            .post(format!("{}/machines/{}/heartbeat", self.base_url, id));
        self.request(req).await?;
        Ok(())
    }

    /// Long-poll for unread messages across all agents on this machine.
    /// Server holds the connection up to `timeout` seconds.
    pub async fn poll_machine(
        &self,
        machine_id: &str,
        timeout: f64,
    ) -> Result<Vec<crate::db::MachineInboxMessage>> {
        let url = format!(
            "{}/machines/{}/poll?timeout={}",
            self.base_url, machine_id, timeout
        );
        let req = self
            .client
            .get(&url)
            .timeout(std::time::Duration::from_secs_f64(timeout + 5.0));
        let resp = self.request(req).await?;
        let body: MachineInboxResponse = resp.json().await?;
        Ok(body.messages)
    }

    // --- Workspaces ---

    pub async fn create_workspace(&self, name: &str) -> Result<crate::db::Workspace> {
        let req = self
            .client
            .post(format!("{}/workspaces", self.base_url))
            .json(&serde_json::json!({"name": name}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_workspaces(&self) -> Result<Vec<crate::db::Workspace>> {
        let req = self.client.get(format!("{}/workspaces", self.base_url));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(body["workspaces"].clone()).unwrap_or_default())
    }

    pub async fn add_workspace_member(&self, workspace: &str, user_id: &str) -> Result<()> {
        let req = self.client.post(format!(
            "{}/workspaces/{}/members/{}",
            self.base_url, workspace, user_id
        ));
        self.request(req).await?;
        Ok(())
    }

    // --- Users ---

    pub async fn invite_user(&self, name: &str) -> Result<InviteResponse> {
        let req = self
            .client
            .post(format!("{}/users/invite", self.base_url))
            .json(&serde_json::json!({"name": name}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    // --- Cron Jobs ---

    pub async fn create_cron_job(
        &self,
        workspace: &str,
        agent: &str,
        schedule: &str,
        task: &str,
        end_date: Option<&str>,
    ) -> Result<crate::db::CronJob> {
        let mut body = serde_json::json!({"agent": agent, "schedule": schedule, "task": task});
        if let Some(d) = end_date { body["end_date"] = serde_json::json!(d); }
        let req = self.client
            .post(format!("{}/workspaces/{}/cron", self.base_url, workspace))
            .json(&body);
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn list_cron_jobs(&self, workspace: &str) -> Result<Vec<crate::db::CronJob>> {
        let req = self
            .client
            .get(format!("{}/workspaces/{}/cron", self.base_url, workspace));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(body["cron_jobs"].clone()).unwrap_or_default())
    }

    pub async fn remove_cron_job(&self, workspace: &str, cron_id: &str) -> Result<()> {
        let req = self.client.delete(format!(
            "{}/workspaces/{}/cron/{}",
            self.base_url, workspace, cron_id
        ));
        self.request(req).await?;
        Ok(())
    }

    pub async fn set_cron_enabled(
        &self,
        workspace: &str,
        cron_id: &str,
        enabled: bool,
    ) -> Result<()> {
        let req = self
            .client
            .put(format!(
                "{}/workspaces/{}/cron/{}",
                self.base_url, workspace, cron_id
            ))
            .json(&serde_json::json!({"enabled": enabled}));
        self.request(req).await?;
        Ok(())
    }

    // --- Threads ---

    pub async fn list_threads(
        &self,
        workspace: &str,
        from_id: &str,
        limit: i64,
    ) -> Result<Vec<crate::db::ThreadSummary>> {
        let req = self.client.get(format!(
            "{}/workspaces/{}/threads?from_id={}&limit={}",
            self.base_url, workspace, from_id, limit
        ));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(body["threads"].clone()).unwrap_or_default())
    }

    // --- Tasks ---

    pub async fn create_task(&self, workspace: &str, title: &str) -> Result<crate::db::Task> {
        let req = self
            .client
            .post(format!("{}/workspaces/{}/tasks", self.base_url, workspace))
            .json(&serde_json::json!({"title": title}));
        let resp = self.request(req).await?;
        Ok(resp.json().await?)
    }

    pub async fn get_agent_for_thread(
        &self,
        workspace: &str,
        thread_id: &str,
    ) -> Result<Option<String>> {
        let req = self.client.get(format!(
            "{}/workspaces/{}/threads/{}",
            self.base_url, workspace, thread_id
        ));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        let messages = body["messages"].as_array();
        if let Some(msgs) = messages {
            for m in msgs {
                if m["type"].as_str() == Some("request") {
                    if let Some(to) = m["to"].as_str() {
                        return Ok(Some(to.to_string()));
                    }
                }
            }
        }
        Ok(None)
    }

    pub async fn list_tasks(&self, workspace: &str) -> Result<Vec<crate::db::Task>> {
        let req = self
            .client
            .get(format!("{}/workspaces/{}/tasks", self.base_url, workspace));
        let resp = self.request(req).await?;
        let body: serde_json::Value = resp.json().await?;
        Ok(serde_json::from_value(body["tasks"].clone()).unwrap_or_default())
    }
}
