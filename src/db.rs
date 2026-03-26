use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use uuid::Uuid;

pub struct Database {
    conn: Mutex<Connection>,
}

pub const DEFAULT_AGENT_TIMEOUT_SECS: i64 = 1800;

// --- Models ---

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct User {
    pub id: String,
    pub name: String,
    pub is_admin: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub name: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub id: String,
    pub thread_id: String,
    #[serde(rename = "from")]
    pub from_id: String,
    #[serde(rename = "to")]
    pub to_id: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub content: Option<serde_json::Value>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// Inbox message enriched with workspace and agent metadata, for machine-level polling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MachineInboxMessage {
    pub id: String,
    pub workspace: String,
    pub thread_id: String,
    #[serde(rename = "from")]
    pub from_id: String,
    #[serde(rename = "to")]
    pub to_id: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub content: Option<serde_json::Value>,
    pub status: String,
    pub created_at: DateTime<Utc>,
    pub agent_instructions: Option<String>,
    pub agent_runtime: Option<String>,
    pub agent_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub machine_id: String,
    pub runtime: String,
    pub status: String,
    pub registered_by: String,
    #[serde(default = "default_kind")]
    pub kind: String,
    #[serde(default = "default_timeout")]
    pub timeout: i64,
    #[serde(default)]
    pub webhook_url: Option<String>,
    #[serde(default)]
    pub slack_channel: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThreadSummary {
    pub thread_id: String,
    pub agent: String,
    pub last_status: String,
    pub first_message: String,
    pub last_activity: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentThreadSummary {
    pub thread_id: String,
    pub first_content: String,
    pub latest_type: String,
    pub latest_at: DateTime<Utc>,
}

fn default_kind() -> String {
    "background".to_string()
}

fn default_timeout() -> i64 {
    DEFAULT_AGENT_TIMEOUT_SECS
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: String,
    pub workspace_name: String,
    pub title: String,
    pub status: String,
    pub agent_name: String,
    pub thread_id: String,
    pub parent_task_id: Option<String>,
    pub result: Option<String>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workflow {
    pub id: String,
    pub workspace_name: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub workflow_id: String,
    pub kind: String,
    pub title: String,
    pub prompt: String,
    pub agent_name: Option<String>,
    pub position_x: f64,
    pub position_y: f64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub id: String,
    pub workflow_id: String,
    pub source_node_id: String,
    pub target_node_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowDefinition {
    pub workflow: Workflow,
    pub nodes: Vec<WorkflowNode>,
    pub edges: Vec<WorkflowEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub workspace_name: String,
    pub name: String,
    pub description: String,
    pub status: String,
    pub node_count: i64,
    pub agent_count: i64,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRun {
    pub id: String,
    pub workflow_id: String,
    pub workspace_name: String,
    pub status: String,
    pub input: Option<String>,
    pub definition_snapshot: Option<String>,
    pub started_by: String,
    pub started_at: DateTime<Utc>,
    pub finished_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowStepRun {
    pub id: String,
    pub workflow_run_id: String,
    pub node_id: String,
    pub node_kind: String,
    pub node_title: String,
    pub node_prompt: String,
    pub agent_name: Option<String>,
    pub thread_id: Option<String>,
    pub status: String,
    pub input: Option<String>,
    pub output: Option<String>,
    pub error: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WorkflowNodeDraft {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub prompt: String,
    pub agent_name: Option<String>,
    pub position_x: f64,
    pub position_y: f64,
}

#[derive(Debug, Clone)]
pub struct WorkflowEdgeDraft {
    pub id: String,
    pub source_node_id: String,
    pub target_node_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Machine {
    pub id: String,
    pub owner: String,
    pub status: String,
    pub last_heartbeat: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CronJob {
    pub id: String,
    pub workspace_name: String,
    pub agent: String,
    pub schedule: String,
    pub task: String,
    pub enabled: bool,
    pub last_run: Option<DateTime<Utc>>,
    pub end_date: Option<DateTime<Utc>>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let conn =
            Connection::open(path).with_context(|| format!("failed to open database: {}", path))?;

        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA synchronous=NORMAL;")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;

        let db = Database {
            conn: Mutex::new(conn),
        };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS users (
                id TEXT NOT NULL PRIMARY KEY,
                name TEXT NOT NULL,
                key TEXT NOT NULL UNIQUE,
                is_admin INTEGER NOT NULL DEFAULT 0,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS workspaces (
                name TEXT NOT NULL PRIMARY KEY,
                created_by TEXT NOT NULL,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS workspace_members (
                workspace_name TEXT NOT NULL,
                user_id TEXT NOT NULL,
                PRIMARY KEY (workspace_name, user_id),
                FOREIGN KEY (workspace_name) REFERENCES workspaces(name),
                FOREIGN KEY (user_id) REFERENCES users(id)
            );

            CREATE TABLE IF NOT EXISTS inbox_messages (
                id TEXT PRIMARY KEY,
                workspace_name TEXT NOT NULL,
                thread_id TEXT NOT NULL,
                from_id TEXT NOT NULL,
                to_id TEXT NOT NULL,
                type TEXT NOT NULL,
                content TEXT,
                status TEXT NOT NULL DEFAULT 'unread',
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS machines (
                id TEXT NOT NULL PRIMARY KEY,
                owner TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'online',
                last_heartbeat TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                FOREIGN KEY (owner) REFERENCES users(id)
            );

            CREATE TABLE IF NOT EXISTS agents (
                workspace_name TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                instructions TEXT NOT NULL,
                machine_id TEXT NOT NULL DEFAULT 'local',
                runtime TEXT NOT NULL DEFAULT 'auto',
                status TEXT NOT NULL DEFAULT 'active',
                registered_by TEXT NOT NULL DEFAULT '',
                kind TEXT NOT NULL DEFAULT 'background',
                timeout INTEGER NOT NULL DEFAULT 1800,
                webhook_url TEXT,
                slack_channel TEXT,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                PRIMARY KEY (workspace_name, name)
            );

            CREATE INDEX IF NOT EXISTS idx_inbox_workspace_to_status ON inbox_messages(workspace_name, to_id, status);
            CREATE INDEX IF NOT EXISTS idx_inbox_workspace_thread ON inbox_messages(workspace_name, thread_id);

            CREATE TABLE IF NOT EXISTS cron_jobs (
                id TEXT PRIMARY KEY,
                workspace_name TEXT NOT NULL,
                agent TEXT NOT NULL,
                schedule TEXT NOT NULL,
                task TEXT NOT NULL,
                enabled INTEGER NOT NULL DEFAULT 1,
                last_run TEXT,
                end_date TEXT,
                created_by TEXT NOT NULL,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                workspace_name TEXT NOT NULL,
                title TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'running',
                agent_name TEXT NOT NULL,
                thread_id TEXT NOT NULL,
                parent_task_id TEXT,
                result TEXT,
                created_by TEXT NOT NULL,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );
            CREATE INDEX IF NOT EXISTS idx_tasks_workspace_status ON tasks(workspace_name, status);
            CREATE INDEX IF NOT EXISTS idx_tasks_parent ON tasks(parent_task_id);

            CREATE TABLE IF NOT EXISTS workflows (
                id TEXT PRIMARY KEY,
                workspace_name TEXT NOT NULL,
                name TEXT NOT NULL,
                description TEXT NOT NULL DEFAULT '',
                status TEXT NOT NULL DEFAULT 'draft',
                created_by TEXT NOT NULL,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );
            CREATE INDEX IF NOT EXISTS idx_workflows_workspace_status ON workflows(workspace_name, status);

            CREATE TABLE IF NOT EXISTS workflow_nodes (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                title TEXT NOT NULL,
                prompt TEXT NOT NULL DEFAULT '',
                agent_name TEXT,
                position_x REAL NOT NULL DEFAULT 0,
                position_y REAL NOT NULL DEFAULT 0,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );
            CREATE INDEX IF NOT EXISTS idx_workflow_nodes_workflow ON workflow_nodes(workflow_id);

            CREATE TABLE IF NOT EXISTS workflow_edges (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                source_node_id TEXT NOT NULL,
                target_node_id TEXT NOT NULL,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                UNIQUE(workflow_id, source_node_id, target_node_id)
            );
            CREATE INDEX IF NOT EXISTS idx_workflow_edges_workflow ON workflow_edges(workflow_id);

            CREATE TABLE IF NOT EXISTS workflow_runs (
                id TEXT PRIMARY KEY,
                workflow_id TEXT NOT NULL,
                workspace_name TEXT NOT NULL,
                status TEXT NOT NULL DEFAULT 'queued',
                input TEXT,
                definition_snapshot TEXT,
                started_by TEXT NOT NULL,
                started_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                finished_at TEXT,
                error TEXT
            );
            CREATE INDEX IF NOT EXISTS idx_workflow_runs_workspace_workflow ON workflow_runs(workspace_name, workflow_id, started_at);

            CREATE TABLE IF NOT EXISTS workflow_step_runs (
                id TEXT PRIMARY KEY,
                workflow_run_id TEXT NOT NULL,
                node_id TEXT NOT NULL,
                node_kind TEXT NOT NULL,
                node_title TEXT NOT NULL,
                node_prompt TEXT NOT NULL DEFAULT '',
                agent_name TEXT,
                thread_id TEXT,
                status TEXT NOT NULL DEFAULT 'pending',
                input TEXT,
                output TEXT,
                error TEXT,
                started_at TEXT,
                finished_at TEXT,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );
            CREATE INDEX IF NOT EXISTS idx_workflow_step_runs_run ON workflow_step_runs(workflow_run_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_workflow_step_runs_thread_id ON workflow_step_runs(thread_id);
            ",
        )?;

        // Migrations for existing databases
        let _ = conn.execute(
            "ALTER TABLE agents ADD COLUMN timeout INTEGER NOT NULL DEFAULT 1800",
            [],
        );
        let _ = conn.execute(
            "ALTER TABLE agents ADD COLUMN kind TEXT NOT NULL DEFAULT 'background'",
            [],
        );
        let _ = conn.execute("ALTER TABLE agents ADD COLUMN webhook_url TEXT", []);
        let _ = conn.execute("ALTER TABLE agents ADD COLUMN slack_channel TEXT", []);
        let _ = conn.execute("ALTER TABLE cron_jobs ADD COLUMN end_date TEXT", []);
        // Migrate old temp column to kind
        let _ = conn.execute("UPDATE agents SET kind = 'temp' WHERE temp = 1", []);
        // Existing databases were created before timeout was configurable and all
        // agents inherited the old 300s default. Raise them to the new default.
        let _ = conn.execute(
            "UPDATE agents SET timeout = ?1 WHERE timeout = 300",
            params![DEFAULT_AGENT_TIMEOUT_SECS],
        );
        // Add definition_snapshot column for workflow runs
        let _ = conn.execute(
            "ALTER TABLE workflow_runs ADD COLUMN definition_snapshot TEXT",
            [],
        );

        Ok(())
    }

    fn parse_ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .map(|dt| dt.with_timezone(&Utc))
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%SZ")
                    .map(|ndt| ndt.and_utc())
            })
            .or_else(|_| {
                chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S")
                    .map(|ndt| ndt.and_utc())
            })
            .unwrap_or_else(|_| Utc::now())
    }

    fn parse_optional_ts(value: Option<String>) -> Option<DateTime<Utc>> {
        value.as_deref().map(Self::parse_ts)
    }

    // --- Users ---

    pub fn create_user(&self, name: &str, is_admin: bool) -> Result<(User, String)> {
        let conn = self.conn.lock().unwrap();
        let id = format!("u-{}", &Uuid::new_v4().to_string()[..8]);
        let key = format!("b0_{}", &Uuid::new_v4().to_string().replace('-', ""));

        conn.execute(
            "INSERT INTO users (id, name, key, is_admin) VALUES (?1, ?2, ?3, ?4)",
            params![id, name, key, is_admin as i32],
        )?;

        // Auto-create personal workspace
        conn.execute(
            "INSERT INTO workspaces (name, created_by) VALUES (?1, ?2)",
            params![name, id],
        )?;
        conn.execute(
            "INSERT INTO workspace_members (workspace_name, user_id) VALUES (?1, ?2)",
            params![name, id],
        )?;

        let ts: String = conn.query_row(
            "SELECT created_at FROM users WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;

        Ok((
            User {
                id,
                name: name.to_string(),
                is_admin,
                created_at: Self::parse_ts(&ts),
            },
            key,
        ))
    }

    /// Validate a key. Returns the User if valid.
    pub fn authenticate(&self, key: &str) -> Result<Option<User>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, is_admin, created_at FROM users WHERE key = ?1",
            params![key],
            |row| {
                let ts: String = row.get(3)?;
                Ok(User {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    is_admin: row.get::<_, i32>(2)? != 0,
                    created_at: Database::parse_ts(&ts),
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn get_admin_user_id(&self) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id FROM users WHERE is_admin = 1 LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn list_users(&self) -> Result<Vec<User>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT id, name, is_admin, created_at FROM users ORDER BY created_at ASC")?;
        let users = stmt
            .query_map([], |row| {
                let ts: String = row.get(3)?;
                Ok(User {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    is_admin: row.get::<_, i32>(2)? != 0,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(users)
    }

    /// Bootstrap: create admin user if none exists. Returns (user, key) if created.
    pub fn bootstrap_admin(&self) -> Result<Option<(User, String)>> {
        let conn = self.conn.lock().unwrap();
        let has_admin: bool = conn
            .query_row("SELECT COUNT(*) FROM users WHERE is_admin = 1", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|c| c > 0)?;

        if has_admin {
            return Ok(None);
        }
        drop(conn);
        Ok(Some(self.create_user("admin", true)?))
    }

    // --- Workspaces ---

    pub fn create_workspace(&self, name: &str, created_by: &str) -> Result<Workspace> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO workspaces (name, created_by) VALUES (?1, ?2)",
            params![name, created_by],
        )?;
        // Creator is automatically a member
        conn.execute(
            "INSERT INTO workspace_members (workspace_name, user_id) VALUES (?1, ?2)",
            params![name, created_by],
        )?;
        let ts: String = conn.query_row(
            "SELECT created_at FROM workspaces WHERE name = ?1",
            params![name],
            |row| row.get(0),
        )?;
        Ok(Workspace {
            name: name.to_string(),
            created_by: created_by.to_string(),
            created_at: Self::parse_ts(&ts),
        })
    }

    pub fn add_workspace_member(&self, workspace_name: &str, user_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO workspace_members (workspace_name, user_id) VALUES (?1, ?2)",
            params![workspace_name, user_id],
        )?;
        Ok(())
    }

    pub fn list_workspaces_for_user(&self, user_id: &str) -> Result<Vec<Workspace>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT w.name, w.created_by, w.created_at FROM workspaces w
             JOIN workspace_members wm ON w.name = wm.workspace_name
             WHERE wm.user_id = ?1 ORDER BY w.name ASC",
        )?;
        let workspaces = stmt
            .query_map(params![user_id], |row| {
                let ts: String = row.get(2)?;
                Ok(Workspace {
                    name: row.get(0)?,
                    created_by: row.get(1)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(workspaces)
    }

    pub fn is_workspace_member(&self, workspace_name: &str, user_id: &str) -> Result<bool> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT COUNT(*) FROM workspace_members WHERE workspace_name = ?1 AND user_id = ?2",
            params![workspace_name, user_id],
            |row| row.get::<_, i64>(0),
        )
        .map(|c| c > 0)
        .map_err(Into::into)
    }

    // --- Inbox Messages ---

    pub fn send_inbox_message(
        &self,
        workspace_name: &str,
        thread_id: &str,
        from: &str,
        to: &str,
        msg_type: &str,
        content: Option<&serde_json::Value>,
    ) -> Result<InboxMessage> {
        let conn = self.conn.lock().unwrap();
        let msg_id = format!("imsg-{}", &Uuid::new_v4().to_string()[..16]);
        let content_str = content.map(|c| serde_json::to_string(c).unwrap_or_default());

        conn.execute(
            "INSERT INTO inbox_messages (id, workspace_name, thread_id, from_id, to_id, type, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![msg_id, workspace_name, thread_id, from, to, msg_type, content_str],
        )?;

        let ts: String = conn.query_row(
            "SELECT created_at FROM inbox_messages WHERE id = ?1",
            params![msg_id],
            |row| row.get(0),
        )?;

        Ok(InboxMessage {
            id: msg_id,
            thread_id: thread_id.to_string(),
            from_id: from.to_string(),
            to_id: to.to_string(),
            msg_type: msg_type.to_string(),
            content: content.cloned(),
            status: "unread".to_string(),
            created_at: Self::parse_ts(&ts),
        })
    }

    pub fn get_inbox_messages(
        &self,
        workspace_name: &str,
        to_id: &str,
        status: Option<&str>,
        thread_id: Option<&str>,
    ) -> Result<Vec<InboxMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut query =
            "SELECT id, thread_id, from_id, to_id, type, content, status, created_at FROM inbox_messages WHERE workspace_name = ?1 AND to_id = ?2"
                .to_string();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![
            Box::new(workspace_name.to_string()),
            Box::new(to_id.to_string()),
        ];
        let mut param_idx = 3;

        if let Some(s) = status {
            query += &format!(" AND status = ?{}", param_idx);
            param_values.push(Box::new(s.to_string()));
            param_idx += 1;
        }
        if let Some(t) = thread_id {
            query += &format!(" AND thread_id = ?{}", param_idx);
            param_values.push(Box::new(t.to_string()));
        }
        query += " ORDER BY created_at ASC";

        let mut stmt = conn.prepare(&query)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            param_values.iter().map(|p| p.as_ref()).collect();
        let messages = stmt
            .query_map(params_ref.as_slice(), |row| {
                let content_str: Option<String> = row.get(5)?;
                let ts: String = row.get(7)?;
                Ok(InboxMessage {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    from_id: row.get(2)?,
                    to_id: row.get(3)?,
                    msg_type: row.get(4)?,
                    content: content_str.and_then(|s| serde_json::from_str(&s).ok()),
                    status: row.get(6)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    pub fn ack_inbox_message(&self, workspace_name: &str, message_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE inbox_messages SET status = 'acked' WHERE id = ?1 AND workspace_name = ?2 AND status = 'unread'",
            params![message_id, workspace_name],
        )?;
        if updated == 0 {
            anyhow::bail!("message not found or already acked");
        }
        Ok(())
    }

    pub fn get_thread_messages(
        &self,
        workspace_name: &str,
        thread_id: &str,
    ) -> Result<Vec<InboxMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, from_id, to_id, type, content, status, created_at
             FROM inbox_messages WHERE workspace_name = ?1 AND thread_id = ?2 ORDER BY created_at ASC",
        )?;
        let messages = stmt
            .query_map(params![workspace_name, thread_id], |row| {
                let content_str: Option<String> = row.get(5)?;
                let ts: String = row.get(7)?;
                Ok(InboxMessage {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    from_id: row.get(2)?,
                    to_id: row.get(3)?,
                    msg_type: row.get(4)?,
                    content: content_str.and_then(|s| serde_json::from_str(&s).ok()),
                    status: row.get(6)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    // --- Machines ---

    pub fn register_machine(&self, id: &str, owner: &str) -> Result<Machine> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR REPLACE INTO machines (id, owner, status, last_heartbeat) VALUES (?1, ?2, 'online', strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))",
            params![id, owner],
        )?;
        Ok(Machine {
            id: id.to_string(),
            owner: owner.to_string(),
            status: "online".to_string(),
            last_heartbeat: Some(Utc::now()),
        })
    }

    pub fn list_machines(&self) -> Result<Vec<Machine>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt =
            conn.prepare("SELECT id, owner, status, last_heartbeat FROM machines ORDER BY id ASC")?;
        let machines = stmt
            .query_map([], |row| {
                let hb: Option<String> = row.get(3)?;
                Ok(Machine {
                    id: row.get(0)?,
                    owner: row.get(1)?,
                    status: row.get(2)?,
                    last_heartbeat: hb.map(|s| Database::parse_ts(&s)),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(machines)
    }

    pub fn get_machine_owner(&self, machine_id: &str) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT owner FROM machines WHERE id = ?1",
            params![machine_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn heartbeat_machine(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE machines SET last_heartbeat = strftime('%Y-%m-%dT%H:%M:%SZ', 'now'), status = 'online' WHERE id = ?1",
            params![id],
        )?;
        Ok(())
    }

    // --- Agents ---

    fn parse_agent_row(row: &rusqlite::Row) -> rusqlite::Result<Agent> {
        let ts: String = row.get(11)?;
        Ok(Agent {
            name: row.get(0)?,
            description: row.get(1)?,
            instructions: row.get(2)?,
            machine_id: row.get(3)?,
            runtime: row.get(4)?,
            status: row.get(5)?,
            registered_by: row.get(6)?,
            kind: row.get(7)?,
            timeout: row.get(8)?,
            webhook_url: row.get(9)?,
            slack_channel: row.get(10)?,
            created_at: Database::parse_ts(&ts),
        })
    }

    const AGENT_COLS: &str = "name, description, instructions, machine_id, runtime, status, registered_by, kind, timeout, webhook_url, slack_channel, created_at";

    pub fn register_agent(
        &self,
        workspace_name: &str,
        name: &str,
        description: &str,
        instructions: &str,
        machine_id: &str,
        runtime: &str,
        registered_by: &str,
        kind: &str,
        webhook_url: Option<&str>,
        slack_channel: Option<&str>,
    ) -> Result<Agent> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO agents (workspace_name, name, description, instructions, machine_id, runtime, registered_by, kind, timeout, webhook_url, slack_channel) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
            params![
                workspace_name,
                name,
                description,
                instructions,
                machine_id,
                runtime,
                registered_by,
                kind,
                DEFAULT_AGENT_TIMEOUT_SECS,
                webhook_url,
                slack_channel,
            ],
        )?;

        let ts: String = conn.query_row(
            "SELECT created_at FROM agents WHERE workspace_name = ?1 AND name = ?2",
            params![workspace_name, name],
            |row| row.get(0),
        )?;

        Ok(Agent {
            name: name.to_string(),
            description: description.to_string(),
            instructions: instructions.to_string(),
            machine_id: machine_id.to_string(),
            runtime: runtime.to_string(),
            status: "active".to_string(),
            registered_by: registered_by.to_string(),
            kind: kind.to_string(),
            timeout: DEFAULT_AGENT_TIMEOUT_SECS,
            webhook_url: webhook_url.map(|s| s.to_string()),
            slack_channel: slack_channel.map(|s| s.to_string()),
            created_at: Self::parse_ts(&ts),
        })
    }

    pub fn list_agents(&self, workspace_name: &str) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            &format!("SELECT {} FROM agents WHERE workspace_name = ?1 AND kind = 'background' ORDER BY created_at ASC", Self::AGENT_COLS),
        )?;
        let agents = stmt
            .query_map(params![workspace_name], Self::parse_agent_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    pub fn get_agent(&self, workspace_name: &str, name: &str) -> Result<Option<Agent>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            &format!(
                "SELECT {} FROM agents WHERE workspace_name = ?1 AND name = ?2",
                Self::AGENT_COLS
            ),
            params![workspace_name, name],
            Self::parse_agent_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn remove_agent(&self, workspace_name: &str, name: &str, user_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        if !user_id.is_empty() {
            let owner: String = conn
                .query_row(
                    "SELECT registered_by FROM agents WHERE workspace_name = ?1 AND name = ?2",
                    params![workspace_name, name],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("agent not found"))?;

            if owner != user_id {
                anyhow::bail!("permission denied: agent was created by someone else");
            }
        }

        let deleted = conn.execute(
            "DELETE FROM agents WHERE workspace_name = ?1 AND name = ?2",
            params![workspace_name, name],
        )?;
        if deleted == 0 {
            anyhow::bail!("agent not found");
        }

        Ok(())
    }

    pub fn update_agent_instructions(
        &self,
        workspace_name: &str,
        name: &str,
        instructions: &str,
        user_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        if !user_id.is_empty() {
            let owner: String = conn
                .query_row(
                    "SELECT registered_by FROM agents WHERE workspace_name = ?1 AND name = ?2",
                    params![workspace_name, name],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("agent not found"))?;

            if owner != user_id {
                anyhow::bail!("permission denied: agent was created by someone else");
            }
        }

        let updated = conn.execute(
            "UPDATE agents SET instructions = ?3 WHERE workspace_name = ?1 AND name = ?2",
            params![workspace_name, name, instructions],
        )?;
        if updated == 0 {
            anyhow::bail!("agent not found");
        }
        Ok(())
    }

    pub fn set_agent_status(
        &self,
        workspace_name: &str,
        name: &str,
        status: &str,
        user_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        if !user_id.is_empty() {
            let owner: String = conn
                .query_row(
                    "SELECT registered_by FROM agents WHERE workspace_name = ?1 AND name = ?2",
                    params![workspace_name, name],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("agent not found"))?;

            if owner != user_id {
                anyhow::bail!("permission denied: agent was created by someone else");
            }
        }

        let updated = conn.execute(
            "UPDATE agents SET status = ?3 WHERE workspace_name = ?1 AND name = ?2",
            params![workspace_name, name, status],
        )?;
        if updated == 0 {
            anyhow::bail!("agent not found");
        }
        Ok(())
    }

    /// Get all active agents on a machine across ALL workspaces.
    /// Used by the daemon.
    pub fn get_all_active_agents_for_machine(
        &self,
        machine_id: &str,
    ) -> Result<Vec<(String, Agent)>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT workspace_name, name, description, instructions, machine_id, runtime, status, registered_by, kind, timeout, webhook_url, slack_channel, created_at FROM agents WHERE machine_id = ?1 AND status = 'active'",
        )?;
        let agents = stmt
            .query_map(params![machine_id], |row| {
                let workspace: String = row.get(0)?;
                let ts: String = row.get(12)?;
                Ok((
                    workspace,
                    Agent {
                        name: row.get(1)?,
                        description: row.get(2)?,
                        instructions: row.get(3)?,
                        machine_id: row.get(4)?,
                        runtime: row.get(5)?,
                        status: row.get(6)?,
                        registered_by: row.get(7)?,
                        kind: row.get(8)?,
                        timeout: row.get(9)?,
                        webhook_url: row.get(10)?,
                        slack_channel: row.get(11)?,
                        created_at: Database::parse_ts(&ts),
                    },
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    /// Get all unread request/answer messages for agents on a given machine.
    /// Joins with agents table to include instructions, runtime, and timeout.
    pub fn get_unread_messages_for_machine(
        &self,
        machine_id: &str,
    ) -> Result<Vec<MachineInboxMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT m.id, m.workspace_name, m.thread_id, m.from_id, m.to_id, m.type, m.content, m.status, m.created_at, a.instructions, a.runtime, a.timeout
             FROM inbox_messages m
             JOIN agents a ON a.name = m.to_id AND a.workspace_name = m.workspace_name
             WHERE a.machine_id = ?1 AND a.status = 'active' AND m.status = 'unread'
               AND m.type IN ('request', 'answer')
             ORDER BY m.created_at ASC",
        )?;
        let messages = stmt
            .query_map(params![machine_id], |row| {
                let content_str: Option<String> = row.get(6)?;
                let ts: String = row.get(8)?;
                let timeout_i: i64 = row.get(11)?;
                Ok(MachineInboxMessage {
                    id: row.get(0)?,
                    workspace: row.get(1)?,
                    thread_id: row.get(2)?,
                    from_id: row.get(3)?,
                    to_id: row.get(4)?,
                    msg_type: row.get(5)?,
                    content: content_str.and_then(|s| serde_json::from_str(&s).ok()),
                    status: row.get(7)?,
                    created_at: Database::parse_ts(&ts),
                    agent_instructions: row.get(9)?,
                    agent_runtime: row.get(10)?,
                    agent_timeout: if timeout_i > 0 {
                        Some(timeout_i as u64)
                    } else {
                        None
                    },
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    pub fn get_agent_logs(
        &self,
        workspace_name: &str,
        agent_name: &str,
        limit: i64,
    ) -> Result<Vec<InboxMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, thread_id, from_id, to_id, type, content, status, created_at
             FROM inbox_messages
             WHERE workspace_name = ?1 AND (to_id = ?2 OR from_id = ?2)
             ORDER BY created_at DESC LIMIT ?3",
        )?;
        let messages = stmt
            .query_map(params![workspace_name, agent_name, limit], |row| {
                let content_str: Option<String> = row.get(5)?;
                let ts: String = row.get(7)?;
                Ok(InboxMessage {
                    id: row.get(0)?,
                    thread_id: row.get(1)?,
                    from_id: row.get(2)?,
                    to_id: row.get(3)?,
                    msg_type: row.get(4)?,
                    content: content_str.and_then(|s| serde_json::from_str(&s).ok()),
                    status: row.get(6)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    // --- Tasks ---

    pub fn create_task(
        &self,
        workspace_name: &str,
        title: &str,
        agent_name: &str,
        thread_id: &str,
        parent_task_id: Option<&str>,
        created_by: &str,
    ) -> Result<Task> {
        let conn = self.conn.lock().unwrap();
        let id = format!("task-{}", &Uuid::new_v4().to_string()[..8]);
        conn.execute(
            "INSERT INTO tasks (id, workspace_name, title, agent_name, thread_id, parent_task_id, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, workspace_name, title, agent_name, thread_id, parent_task_id, created_by],
        )?;
        let ts: String = conn.query_row(
            "SELECT created_at FROM tasks WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(Task {
            id,
            workspace_name: workspace_name.to_string(),
            title: title.to_string(),
            status: "running".to_string(),
            agent_name: agent_name.to_string(),
            thread_id: thread_id.to_string(),
            parent_task_id: parent_task_id.map(|s| s.to_string()),
            result: None,
            created_by: created_by.to_string(),
            created_at: Self::parse_ts(&ts),
        })
    }

    pub fn get_task(&self, workspace_name: &str, task_id: &str) -> Result<Option<Task>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, workspace_name, title, status, agent_name, thread_id, parent_task_id, result, created_by, created_at FROM tasks WHERE id = ?1 AND workspace_name = ?2",
            params![task_id, workspace_name],
            |row| {
                let ts: String = row.get(9)?;
                Ok(Task {
                    id: row.get(0)?,
                    workspace_name: row.get(1)?,
                    title: row.get(2)?,
                    status: row.get(3)?,
                    agent_name: row.get(4)?,
                    thread_id: row.get(5)?,
                    parent_task_id: row.get(6)?,
                    result: row.get(7)?,
                    created_by: row.get(8)?,
                    created_at: Database::parse_ts(&ts),
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn list_tasks(&self, workspace_name: &str) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workspace_name, title, status, agent_name, thread_id, parent_task_id, result, created_by, created_at FROM tasks WHERE workspace_name = ?1 ORDER BY created_at DESC",
        )?;
        let tasks = stmt
            .query_map(params![workspace_name], |row| {
                let ts: String = row.get(9)?;
                Ok(Task {
                    id: row.get(0)?,
                    workspace_name: row.get(1)?,
                    title: row.get(2)?,
                    status: row.get(3)?,
                    agent_name: row.get(4)?,
                    thread_id: row.get(5)?,
                    parent_task_id: row.get(6)?,
                    result: row.get(7)?,
                    created_by: row.get(8)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    pub fn update_task_status(
        &self,
        workspace_name: &str,
        task_id: &str,
        status: &str,
        result: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE tasks SET status = ?3, result = COALESCE(?4, result) WHERE id = ?1 AND workspace_name = ?2",
            params![task_id, workspace_name, status, result],
        )?;
        Ok(())
    }

    pub fn get_subtasks(&self, workspace_name: &str, parent_task_id: &str) -> Result<Vec<Task>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workspace_name, title, status, agent_name, thread_id, parent_task_id, result, created_by, created_at FROM tasks WHERE workspace_name = ?1 AND parent_task_id = ?2 ORDER BY created_at ASC",
        )?;
        let tasks = stmt
            .query_map(params![workspace_name, parent_task_id], |row| {
                let ts: String = row.get(9)?;
                Ok(Task {
                    id: row.get(0)?,
                    workspace_name: row.get(1)?,
                    title: row.get(2)?,
                    status: row.get(3)?,
                    agent_name: row.get(4)?,
                    thread_id: row.get(5)?,
                    parent_task_id: row.get(6)?,
                    result: row.get(7)?,
                    created_by: row.get(8)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(tasks)
    }

    pub fn get_task_by_thread(
        &self,
        workspace_name: &str,
        thread_id: &str,
    ) -> Result<Option<Task>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, workspace_name, title, status, agent_name, thread_id, parent_task_id, result, created_by, created_at FROM tasks WHERE workspace_name = ?1 AND thread_id = ?2",
            params![workspace_name, thread_id],
            |row| {
                let ts: String = row.get(9)?;
                Ok(Task {
                    id: row.get(0)?,
                    workspace_name: row.get(1)?,
                    title: row.get(2)?,
                    status: row.get(3)?,
                    agent_name: row.get(4)?,
                    thread_id: row.get(5)?,
                    parent_task_id: row.get(6)?,
                    result: row.get(7)?,
                    created_by: row.get(8)?,
                    created_at: Database::parse_ts(&ts),
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    /// Look up the agent name for a thread by finding the first request message's recipient.
    pub fn get_agent_for_thread(
        &self,
        workspace_name: &str,
        thread_id: &str,
    ) -> Result<Option<String>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT to_id FROM inbox_messages WHERE workspace_name = ?1 AND thread_id = ?2 AND type = 'request' ORDER BY created_at ASC LIMIT 1",
            params![workspace_name, thread_id],
            |row| row.get::<_, String>(0),
        )
        .optional()
        .map_err(Into::into)
    }

    /// List recent threads for a user, showing agent, last message type, and time.
    pub fn list_threads(
        &self,
        workspace_name: &str,
        from_id: &str,
        limit: i64,
    ) -> Result<Vec<ThreadSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT m.thread_id, m.to_id,
                    (SELECT type FROM inbox_messages WHERE workspace_name = ?1 AND thread_id = m.thread_id ORDER BY created_at DESC LIMIT 1),
                    (SELECT content FROM inbox_messages WHERE workspace_name = ?1 AND thread_id = m.thread_id AND type = 'request' ORDER BY created_at ASC LIMIT 1),
                    MAX(m.created_at)
             FROM inbox_messages m
             WHERE m.workspace_name = ?1 AND m.from_id = ?2 AND m.type = 'request'
             GROUP BY m.thread_id
             ORDER BY MAX(m.created_at) DESC
             LIMIT ?3",
        )?;
        let threads = stmt
            .query_map(params![workspace_name, from_id, limit], |row| {
                let content_str: Option<String> = row.get(3)?;
                let ts: String = row.get(4)?;
                Ok(ThreadSummary {
                    thread_id: row.get(0)?,
                    agent: row.get(1)?,
                    last_status: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    first_message: content_str
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .and_then(|v| v.as_str().map(|s| s.to_string()))
                        .unwrap_or_default(),
                    last_activity: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(threads)
    }

    pub fn list_agent_threads(
        &self,
        workspace_name: &str,
        agent_name: &str,
        limit: i64,
    ) -> Result<Vec<AgentThreadSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT m.thread_id,
                    (SELECT content FROM inbox_messages
                     WHERE workspace_name = ?1 AND thread_id = m.thread_id AND type = 'request'
                     ORDER BY created_at ASC LIMIT 1),
                    (SELECT type FROM inbox_messages
                     WHERE workspace_name = ?1 AND thread_id = m.thread_id
                     ORDER BY created_at DESC LIMIT 1),
                    MAX(m.created_at)
             FROM inbox_messages m
             WHERE m.workspace_name = ?1 AND m.to_id = ?2 AND m.type = 'request'
             GROUP BY m.thread_id
             ORDER BY MAX(m.created_at) DESC
             LIMIT ?3",
        )?;
        let threads = stmt
            .query_map(params![workspace_name, agent_name, limit], |row| {
                let first_content_str: Option<String> = row.get(1)?;
                let latest_at: String = row.get(3)?;
                Ok(AgentThreadSummary {
                    thread_id: row.get(0)?,
                    first_content: first_content_str
                        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                        .map(|v| match v {
                            serde_json::Value::String(s) => s,
                            other => other.to_string(),
                        })
                        .unwrap_or_default(),
                    latest_type: row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                    latest_at: Database::parse_ts(&latest_at),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(threads)
    }

    /// Delete temp agents older than the given number of seconds.
    /// Returns the number of agents deleted.
    pub fn cleanup_expired_temp_agents(&self, max_age_secs: i64) -> Result<usize> {
        let conn = self.conn.lock().unwrap();
        let cutoff = (chrono::Utc::now() - chrono::Duration::seconds(max_age_secs))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let deleted = conn.execute(
            "DELETE FROM agents WHERE kind = 'temp' AND created_at < ?1",
            params![cutoff],
        )?;
        Ok(deleted)
    }

    // --- Cron Jobs ---

    pub fn create_cron_job(
        &self,
        workspace_name: &str,
        agent: &str,
        schedule: &str,
        task: &str,
        created_by: &str,
        end_date: Option<&str>,
    ) -> Result<CronJob> {
        let conn = self.conn.lock().unwrap();
        let id = format!("cron-{}", &Uuid::new_v4().to_string()[..8]);
        conn.execute(
            "INSERT INTO cron_jobs (id, workspace_name, agent, schedule, task, created_by, end_date) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![id, workspace_name, agent, schedule, task, created_by, end_date],
        )?;
        let parsed_end = end_date.map(|s| Self::parse_ts(s));
        Ok(CronJob {
            id,
            workspace_name: workspace_name.to_string(),
            agent: agent.to_string(),
            schedule: schedule.to_string(),
            task: task.to_string(),
            enabled: true,
            last_run: None,
            end_date: parsed_end,
            created_by: created_by.to_string(),
            created_at: Utc::now(),
        })
    }

    fn parse_cron_row(row: &rusqlite::Row) -> rusqlite::Result<CronJob> {
        let last_run_str: Option<String> = row.get(6)?;
        let end_date_str: Option<String> = row.get(7)?;
        let ts: String = row.get(9)?;
        let enabled: i32 = row.get(5)?;
        Ok(CronJob {
            id: row.get(0)?,
            workspace_name: row.get(1)?,
            agent: row.get(2)?,
            schedule: row.get(3)?,
            task: row.get(4)?,
            enabled: enabled != 0,
            last_run: last_run_str.map(|s| Database::parse_ts(&s)),
            end_date: end_date_str.map(|s| Database::parse_ts(&s)),
            created_by: row.get(8)?,
            created_at: Database::parse_ts(&ts),
        })
    }

    const CRON_COLS: &str = "id, workspace_name, agent, schedule, task, enabled, last_run, end_date, created_by, created_at";

    pub fn list_cron_jobs(&self, workspace_name: &str) -> Result<Vec<CronJob>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            &format!("SELECT {} FROM cron_jobs WHERE workspace_name = ?1 ORDER BY created_at", Self::CRON_COLS),
        )?;
        let jobs = stmt
            .query_map(params![workspace_name], Self::parse_cron_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(jobs)
    }

    pub fn get_all_enabled_cron_jobs(&self) -> Result<Vec<CronJob>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            &format!("SELECT {} FROM cron_jobs WHERE enabled = 1", Self::CRON_COLS),
        )?;
        let jobs = stmt
            .query_map([], Self::parse_cron_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(jobs)
    }

    pub fn remove_cron_job(
        &self,
        workspace_name: &str,
        cron_id: &str,
        user_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let creator: Option<String> = conn
            .query_row(
                "SELECT created_by FROM cron_jobs WHERE id = ?1 AND workspace_name = ?2",
                params![cron_id, workspace_name],
                |row| row.get(0),
            )
            .optional()?;
        match creator {
            Some(c) if c == user_id => {}
            Some(_) => anyhow::bail!("only the creator can remove this cron job"),
            None => anyhow::bail!("cron job not found"),
        }
        conn.execute(
            "DELETE FROM cron_jobs WHERE id = ?1 AND workspace_name = ?2",
            params![cron_id, workspace_name],
        )?;
        Ok(())
    }

    pub fn set_cron_enabled(
        &self,
        workspace_name: &str,
        cron_id: &str,
        enabled: bool,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE cron_jobs SET enabled = ?1 WHERE id = ?2 AND workspace_name = ?3",
            params![enabled as i32, cron_id, workspace_name],
        )?;
        Ok(())
    }

    pub fn update_cron_last_run(&self, cron_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE cron_jobs SET last_run = ?1 WHERE id = ?2",
            params![now, cron_id],
        )?;
        Ok(())
    }

    // --- Workflows ---

    fn parse_workflow_row(row: &rusqlite::Row) -> rusqlite::Result<Workflow> {
        let created_at: String = row.get(6)?;
        let updated_at: String = row.get(7)?;
        Ok(Workflow {
            id: row.get(0)?,
            workspace_name: row.get(1)?,
            name: row.get(2)?,
            description: row.get(3)?,
            status: row.get(4)?,
            created_by: row.get(5)?,
            created_at: Database::parse_ts(&created_at),
            updated_at: Database::parse_ts(&updated_at),
        })
    }

    fn parse_workflow_node_row(row: &rusqlite::Row) -> rusqlite::Result<WorkflowNode> {
        let created_at: String = row.get(8)?;
        let updated_at: String = row.get(9)?;
        Ok(WorkflowNode {
            id: row.get(0)?,
            workflow_id: row.get(1)?,
            kind: row.get(2)?,
            title: row.get(3)?,
            prompt: row.get(4)?,
            agent_name: row.get(5)?,
            position_x: row.get(6)?,
            position_y: row.get(7)?,
            created_at: Database::parse_ts(&created_at),
            updated_at: Database::parse_ts(&updated_at),
        })
    }

    fn parse_workflow_edge_row(row: &rusqlite::Row) -> rusqlite::Result<WorkflowEdge> {
        let created_at: String = row.get(4)?;
        Ok(WorkflowEdge {
            id: row.get(0)?,
            workflow_id: row.get(1)?,
            source_node_id: row.get(2)?,
            target_node_id: row.get(3)?,
            created_at: Database::parse_ts(&created_at),
        })
    }

    fn parse_workflow_run_row(row: &rusqlite::Row) -> rusqlite::Result<WorkflowRun> {
        let started_at: String = row.get(7)?;
        let finished_at: Option<String> = row.get(8)?;
        Ok(WorkflowRun {
            id: row.get(0)?,
            workflow_id: row.get(1)?,
            workspace_name: row.get(2)?,
            status: row.get(3)?,
            input: row.get(4)?,
            definition_snapshot: row.get(5)?,
            started_by: row.get(6)?,
            started_at: Database::parse_ts(&started_at),
            finished_at: Database::parse_optional_ts(finished_at),
            error: row.get(9)?,
        })
    }

    fn parse_workflow_step_run_row(row: &rusqlite::Row) -> rusqlite::Result<WorkflowStepRun> {
        let started_at: Option<String> = row.get(12)?;
        let finished_at: Option<String> = row.get(13)?;
        let created_at: String = row.get(14)?;
        Ok(WorkflowStepRun {
            id: row.get(0)?,
            workflow_run_id: row.get(1)?,
            node_id: row.get(2)?,
            node_kind: row.get(3)?,
            node_title: row.get(4)?,
            node_prompt: row.get(5)?,
            agent_name: row.get(6)?,
            thread_id: row.get(7)?,
            status: row.get(8)?,
            input: row.get(9)?,
            output: row.get(10)?,
            error: row.get(11)?,
            started_at: Database::parse_optional_ts(started_at),
            finished_at: Database::parse_optional_ts(finished_at),
            created_at: Database::parse_ts(&created_at),
        })
    }

    pub fn create_workflow(
        &self,
        workspace_name: &str,
        name: &str,
        description: &str,
        status: &str,
        created_by: &str,
        nodes: &[WorkflowNodeDraft],
        edges: &[WorkflowEdgeDraft],
    ) -> Result<Workflow> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;
        let workflow_id = format!("wf-{}", &Uuid::new_v4().to_string()[..8]);

        tx.execute(
            "INSERT INTO workflows (id, workspace_name, name, description, status, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![workflow_id, workspace_name, name, description, status, created_by],
        )?;

        for node in nodes {
            tx.execute(
                "INSERT INTO workflow_nodes (id, workflow_id, kind, title, prompt, agent_name, position_x, position_y) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params![
                    node.id,
                    workflow_id,
                    node.kind,
                    node.title,
                    node.prompt,
                    node.agent_name,
                    node.position_x,
                    node.position_y,
                ],
            )?;
        }

        for edge in edges {
            tx.execute(
                "INSERT INTO workflow_edges (id, workflow_id, source_node_id, target_node_id) VALUES (?1, ?2, ?3, ?4)",
                params![edge.id, workflow_id, edge.source_node_id, edge.target_node_id],
            )?;
        }

        let row = tx.query_row(
            "SELECT id, workspace_name, name, description, status, created_by, created_at, updated_at FROM workflows WHERE id = ?1",
            params![workflow_id],
            Self::parse_workflow_row,
        )?;
        tx.commit()?;
        Ok(row)
    }

    pub fn list_workflows(&self, workspace_name: &str) -> Result<Vec<WorkflowSummary>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT w.id, w.workspace_name, w.name, w.description, w.status,
                    (SELECT COUNT(*) FROM workflow_nodes n WHERE n.workflow_id = w.id),
                    (SELECT COUNT(DISTINCT n.agent_name) FROM workflow_nodes n WHERE n.workflow_id = w.id AND n.agent_name IS NOT NULL),
                    w.created_by, w.created_at, w.updated_at
             FROM workflows w
             WHERE w.workspace_name = ?1
             ORDER BY w.updated_at DESC, w.created_at DESC",
        )?;
        let workflows = stmt
            .query_map(params![workspace_name], |row| {
                let created_at: String = row.get(8)?;
                let updated_at: String = row.get(9)?;
                Ok(WorkflowSummary {
                    id: row.get(0)?,
                    workspace_name: row.get(1)?,
                    name: row.get(2)?,
                    description: row.get(3)?,
                    status: row.get(4)?,
                    node_count: row.get(5)?,
                    agent_count: row.get(6)?,
                    created_by: row.get(7)?,
                    created_at: Database::parse_ts(&created_at),
                    updated_at: Database::parse_ts(&updated_at),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(workflows)
    }

    pub fn get_workflow(
        &self,
        workspace_name: &str,
        workflow_id: &str,
    ) -> Result<Option<Workflow>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, workspace_name, name, description, status, created_by, created_at, updated_at
             FROM workflows WHERE workspace_name = ?1 AND id = ?2",
            params![workspace_name, workflow_id],
            Self::parse_workflow_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn get_workflow_definition(
        &self,
        workspace_name: &str,
        workflow_id: &str,
    ) -> Result<Option<WorkflowDefinition>> {
        let conn = self.conn.lock().unwrap();
        let workflow = conn
            .query_row(
                "SELECT id, workspace_name, name, description, status, created_by, created_at, updated_at
                 FROM workflows WHERE workspace_name = ?1 AND id = ?2",
                params![workspace_name, workflow_id],
                Self::parse_workflow_row,
            )
            .optional()?;

        let Some(workflow) = workflow else {
            return Ok(None);
        };

        let mut node_stmt = conn.prepare(
            "SELECT id, workflow_id, kind, title, prompt, agent_name, position_x, position_y, created_at, updated_at
             FROM workflow_nodes WHERE workflow_id = ?1 ORDER BY created_at ASC",
        )?;
        let nodes = node_stmt
            .query_map(params![workflow_id], Self::parse_workflow_node_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        let mut edge_stmt = conn.prepare(
            "SELECT id, workflow_id, source_node_id, target_node_id, created_at
             FROM workflow_edges WHERE workflow_id = ?1 ORDER BY created_at ASC",
        )?;
        let edges = edge_stmt
            .query_map(params![workflow_id], Self::parse_workflow_edge_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(Some(WorkflowDefinition {
            workflow,
            nodes,
            edges,
        }))
    }

    pub fn update_workflow(
        &self,
        workspace_name: &str,
        workflow_id: &str,
        name: &str,
        description: &str,
        status: &str,
        user_id: &str,
        nodes: &[WorkflowNodeDraft],
        edges: &[WorkflowEdgeDraft],
    ) -> Result<Workflow> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        if !user_id.is_empty() {
            let owner: String = tx
                .query_row(
                    "SELECT created_by FROM workflows WHERE workspace_name = ?1 AND id = ?2",
                    params![workspace_name, workflow_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("workflow not found"))?;
            if owner != user_id {
                anyhow::bail!("permission denied: workflow was created by someone else");
            }
        }

        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let updated = tx.execute(
            "UPDATE workflows
             SET name = ?3, description = ?4, status = ?5, updated_at = ?6
             WHERE workspace_name = ?1 AND id = ?2",
            params![workspace_name, workflow_id, name, description, status, now],
        )?;
        if updated == 0 {
            anyhow::bail!("workflow not found");
        }

        tx.execute(
            "DELETE FROM workflow_edges WHERE workflow_id = ?1",
            params![workflow_id],
        )?;
        tx.execute(
            "DELETE FROM workflow_nodes WHERE workflow_id = ?1",
            params![workflow_id],
        )?;

        for node in nodes {
            tx.execute(
                "INSERT INTO workflow_nodes (id, workflow_id, kind, title, prompt, agent_name, position_x, position_y, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?9)",
                params![
                    node.id,
                    workflow_id,
                    node.kind,
                    node.title,
                    node.prompt,
                    node.agent_name,
                    node.position_x,
                    node.position_y,
                    now,
                ],
            )?;
        }

        for edge in edges {
            tx.execute(
                "INSERT INTO workflow_edges (id, workflow_id, source_node_id, target_node_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                params![edge.id, workflow_id, edge.source_node_id, edge.target_node_id, now],
            )?;
        }

        let workflow = tx.query_row(
            "SELECT id, workspace_name, name, description, status, created_by, created_at, updated_at
             FROM workflows WHERE workspace_name = ?1 AND id = ?2",
            params![workspace_name, workflow_id],
            Self::parse_workflow_row,
        )?;
        tx.commit()?;
        Ok(workflow)
    }

    pub fn set_workflow_status(
        &self,
        workspace_name: &str,
        workflow_id: &str,
        status: &str,
        user_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();

        if !user_id.is_empty() {
            let owner: String = conn
                .query_row(
                    "SELECT created_by FROM workflows WHERE workspace_name = ?1 AND id = ?2",
                    params![workspace_name, workflow_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("workflow not found"))?;
            if owner != user_id {
                anyhow::bail!("permission denied: workflow was created by someone else");
            }
        }

        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let updated = conn.execute(
            "UPDATE workflows SET status = ?3, updated_at = ?4 WHERE workspace_name = ?1 AND id = ?2",
            params![workspace_name, workflow_id, status, now],
        )?;
        if updated == 0 {
            anyhow::bail!("workflow not found");
        }
        Ok(())
    }

    pub fn remove_workflow(
        &self,
        workspace_name: &str,
        workflow_id: &str,
        user_id: &str,
    ) -> Result<()> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        if !user_id.is_empty() {
            let owner: String = tx
                .query_row(
                    "SELECT created_by FROM workflows WHERE workspace_name = ?1 AND id = ?2",
                    params![workspace_name, workflow_id],
                    |row| row.get(0),
                )
                .optional()?
                .ok_or_else(|| anyhow::anyhow!("workflow not found"))?;
            if owner != user_id {
                anyhow::bail!("permission denied: workflow was created by someone else");
            }
        }

        tx.execute(
            "DELETE FROM workflow_edges WHERE workflow_id = ?1",
            params![workflow_id],
        )?;
        tx.execute(
            "DELETE FROM workflow_nodes WHERE workflow_id = ?1",
            params![workflow_id],
        )?;
        let deleted = tx.execute(
            "DELETE FROM workflows WHERE workspace_name = ?1 AND id = ?2",
            params![workspace_name, workflow_id],
        )?;
        if deleted == 0 {
            anyhow::bail!("workflow not found");
        }

        tx.commit()?;
        Ok(())
    }

    pub fn create_workflow_run(
        &self,
        workspace_name: &str,
        workflow_id: &str,
        input: Option<&str>,
        started_by: &str,
    ) -> Result<WorkflowRun> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction()?;

        let workflow = tx
            .query_row(
                "SELECT id, workspace_name, name, description, status, created_by, created_at, updated_at
                 FROM workflows WHERE workspace_name = ?1 AND id = ?2",
                params![workspace_name, workflow_id],
                Self::parse_workflow_row,
            )
            .optional()?
            .ok_or_else(|| anyhow::anyhow!("workflow not found"))?;

        let nodes = {
            let mut node_stmt = tx.prepare(
                "SELECT id, workflow_id, kind, title, prompt, agent_name, position_x, position_y, created_at, updated_at
                 FROM workflow_nodes WHERE workflow_id = ?1 ORDER BY created_at ASC",
            )?;
            node_stmt
                .query_map(params![workflow_id], Self::parse_workflow_node_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        if nodes.is_empty() {
            anyhow::bail!("workflow has no nodes");
        }

        let edges = {
            let mut edge_stmt = tx.prepare(
                "SELECT id, workflow_id, source_node_id, target_node_id, created_at
                 FROM workflow_edges WHERE workflow_id = ?1",
            )?;
            edge_stmt
                .query_map(params![workflow_id], Self::parse_workflow_edge_row)?
                .collect::<std::result::Result<Vec<_>, _>>()?
        };

        let definition = WorkflowDefinition {
            workflow: workflow.clone(),
            nodes: nodes.clone(),
            edges,
        };
        let snapshot = serde_json::to_string(&definition).ok();

        let run_id = format!("wfr-{}", &Uuid::new_v4().to_string()[..8]);
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        tx.execute(
            "INSERT INTO workflow_runs (id, workflow_id, workspace_name, status, input, definition_snapshot, started_by, started_at)
             VALUES (?1, ?2, ?3, 'queued', ?4, ?5, ?6, ?7)",
            params![run_id, workflow.id, workspace_name, input, snapshot, started_by, now],
        )?;

        for node in nodes {
            let step_id = format!("wfs-{}", &Uuid::new_v4().to_string()[..8]);
            let is_start = node.kind == "start";
            tx.execute(
                "INSERT INTO workflow_step_runs
                 (id, workflow_run_id, node_id, node_kind, node_title, node_prompt, agent_name, status, input, output, started_at, finished_at, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
                params![
                    step_id,
                    run_id,
                    node.id,
                    node.kind,
                    node.title,
                    node.prompt,
                    node.agent_name,
                    if is_start { "done" } else { "pending" },
                    if is_start { input } else { None::<&str> },
                    if is_start { input } else { None::<&str> },
                    if is_start { Some(now.as_str()) } else { None::<&str> },
                    if is_start { Some(now.as_str()) } else { None::<&str> },
                    now,
                ],
            )?;
        }

        let run = tx.query_row(
            "SELECT id, workflow_id, workspace_name, status, input, definition_snapshot, started_by, started_at, finished_at, error
             FROM workflow_runs WHERE id = ?1",
            params![run_id],
            Self::parse_workflow_run_row,
        )?;

        tx.commit()?;
        Ok(run)
    }

    pub fn list_workflow_runs(
        &self,
        workspace_name: &str,
        workflow_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<WorkflowRun>> {
        let conn = self.conn.lock().unwrap();
        let mut query = "SELECT id, workflow_id, workspace_name, status, input, definition_snapshot, started_by, started_at, finished_at, error
                         FROM workflow_runs WHERE workspace_name = ?1"
            .to_string();
        let mut params_vec: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(workspace_name.to_string())];
        let mut idx = 2;
        if let Some(workflow_id) = workflow_id {
            query += &format!(" AND workflow_id = ?{}", idx);
            params_vec.push(Box::new(workflow_id.to_string()));
            idx += 1;
        }
        query += &format!(" ORDER BY started_at DESC LIMIT ?{}", idx);
        params_vec.push(Box::new(limit));
        let mut stmt = conn.prepare(&query)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> =
            params_vec.iter().map(|item| item.as_ref()).collect();
        let runs = stmt
            .query_map(params_ref.as_slice(), Self::parse_workflow_run_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(runs)
    }

    pub fn get_workflow_run(
        &self,
        workspace_name: &str,
        run_id: &str,
    ) -> Result<Option<WorkflowRun>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, workflow_id, workspace_name, status, input, definition_snapshot, started_by, started_at, finished_at, error
             FROM workflow_runs WHERE workspace_name = ?1 AND id = ?2",
            params![workspace_name, run_id],
            Self::parse_workflow_run_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn list_workflow_step_runs(&self, run_id: &str) -> Result<Vec<WorkflowStepRun>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workflow_run_id, node_id, node_kind, node_title, node_prompt, agent_name, thread_id,
                    status, input, output, error, started_at, finished_at, created_at
             FROM workflow_step_runs WHERE workflow_run_id = ?1 ORDER BY created_at ASC",
        )?;
        let steps = stmt
            .query_map(params![run_id], Self::parse_workflow_step_run_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(steps)
    }

    pub fn get_workflow_step_run(
        &self,
        run_id: &str,
        step_run_id: &str,
    ) -> Result<Option<WorkflowStepRun>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, workflow_run_id, node_id, node_kind, node_title, node_prompt, agent_name, thread_id,
                    status, input, output, error, started_at, finished_at, created_at
             FROM workflow_step_runs WHERE workflow_run_id = ?1 AND id = ?2",
            params![run_id, step_run_id],
            Self::parse_workflow_step_run_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn get_workflow_step_run_by_thread(
        &self,
        workspace_name: &str,
        thread_id: &str,
    ) -> Result<Option<WorkflowStepRun>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT s.id, s.workflow_run_id, s.node_id, s.node_kind, s.node_title, s.node_prompt, s.agent_name, s.thread_id,
                    s.status, s.input, s.output, s.error, s.started_at, s.finished_at, s.created_at
             FROM workflow_step_runs s
             JOIN workflow_runs r ON r.id = s.workflow_run_id
             WHERE r.workspace_name = ?1 AND s.thread_id = ?2",
            params![workspace_name, thread_id],
            Self::parse_workflow_step_run_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn dispatch_workflow_step_run(
        &self,
        run_id: &str,
        step_run_id: &str,
        thread_id: &str,
        input: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE workflow_step_runs
             SET status = 'running', thread_id = ?3, input = ?4, error = NULL, output = NULL, started_at = ?5, finished_at = NULL
             WHERE workflow_run_id = ?1 AND id = ?2",
            params![run_id, step_run_id, thread_id, input, now],
        )?;
        Ok(())
    }

    pub fn mark_workflow_step_run_ready(&self, step_run_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE workflow_step_runs SET status = 'ready' WHERE id = ?1 AND status = 'pending'",
            params![step_run_id],
        )?;
        Ok(())
    }

    pub fn mark_workflow_step_run_started(&self, step_run_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE workflow_step_runs
             SET status = 'running', started_at = COALESCE(started_at, ?2)
             WHERE id = ?1",
            params![step_run_id, now],
        )?;
        Ok(())
    }

    pub fn complete_workflow_step_run(
        &self,
        step_run_id: &str,
        status: &str,
        output: Option<&str>,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE workflow_step_runs
             SET status = ?2, output = COALESCE(?3, output), error = ?4, finished_at = ?5
             WHERE id = ?1",
            params![step_run_id, status, output, error, now],
        )?;
        Ok(())
    }

    pub fn set_workflow_step_run_waiting_for_input(&self, step_run_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE workflow_step_runs SET status = 'waiting_for_input' WHERE id = ?1",
            params![step_run_id],
        )?;
        Ok(())
    }

    pub fn set_workflow_step_run_output(
        &self,
        step_run_id: &str,
        status: &str,
        input: Option<&str>,
        output: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE workflow_step_runs
             SET status = ?2, input = COALESCE(?3, input), output = COALESCE(?4, output), error = NULL,
                 started_at = COALESCE(started_at, ?5), finished_at = CASE WHEN ?2 = 'done' THEN ?5 ELSE finished_at END
             WHERE id = ?1",
            params![step_run_id, status, input, output, now],
        )?;
        Ok(())
    }

    pub fn reset_workflow_step_runs(&self, run_id: &str, step_run_ids: &[String]) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        for step_run_id in step_run_ids {
            conn.execute(
                "UPDATE workflow_step_runs
                 SET status = 'pending', thread_id = NULL, input = NULL, output = NULL, error = NULL, started_at = NULL, finished_at = NULL
                 WHERE workflow_run_id = ?1 AND id = ?2",
                params![run_id, step_run_id],
            )?;
        }
        Ok(())
    }

    /// Find workflow runs that have been in a non-terminal state for longer than `max_age_secs`.
    pub fn get_stale_workflow_runs(&self, max_age_secs: i64) -> Result<Vec<WorkflowRun>> {
        let conn = self.conn.lock().unwrap();
        let cutoff = (Utc::now() - chrono::Duration::seconds(max_age_secs))
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string();
        let mut stmt = conn.prepare(
            "SELECT id, workflow_id, workspace_name, status, input, definition_snapshot, started_by, started_at, finished_at, error
             FROM workflow_runs
             WHERE status IN ('queued', 'running', 'waiting_for_input')
               AND started_at < ?1",
        )?;
        let runs = stmt
            .query_map(params![cutoff], Self::parse_workflow_run_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(runs)
    }

    /// Fail all non-terminal step runs in a workflow run and mark the run as failed.
    pub fn timeout_workflow_run(
        &self,
        workspace_name: &str,
        run_id: &str,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string();
        conn.execute(
            "UPDATE workflow_step_runs
             SET status = 'failed', error = 'workflow run timed out', finished_at = ?2
             WHERE workflow_run_id = ?1 AND status IN ('pending', 'ready', 'running', 'waiting_for_input')",
            params![run_id, now],
        )?;
        conn.execute(
            "UPDATE workflow_runs
             SET status = 'failed', error = 'workflow run timed out', finished_at = ?3
             WHERE workspace_name = ?1 AND id = ?2",
            params![workspace_name, run_id, now],
        )?;
        Ok(())
    }

    pub fn update_workflow_run_status(
        &self,
        workspace_name: &str,
        run_id: &str,
        status: &str,
        error: Option<&str>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let finished_at = if matches!(status, "done" | "failed" | "cancelled") {
            Some(Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string())
        } else {
            None
        };
        conn.execute(
            "UPDATE workflow_runs
             SET status = ?3, error = ?4, finished_at = CASE WHEN ?5 IS NULL THEN NULL ELSE ?5 END
             WHERE workspace_name = ?1 AND id = ?2",
            params![workspace_name, run_id, status, error, finished_at],
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_db() -> Database {
        Database::new(":memory:").unwrap()
    }

    #[test]
    fn test_bootstrap_admin() {
        let db = test_db();
        let result = db.bootstrap_admin().unwrap();
        assert!(result.is_some());
        let (user, key) = result.unwrap();
        assert!(user.is_admin);
        assert!(key.starts_with("b0_"));

        // Second call returns None
        assert!(db.bootstrap_admin().unwrap().is_none());
    }

    #[test]
    fn test_create_user_gets_personal_workspace() {
        let db = test_db();
        let (user, _key) = db.create_user("alice", false).unwrap();

        let workspaces = db.list_workspaces_for_user(&user.id).unwrap();
        assert_eq!(workspaces.len(), 1);
        assert_eq!(workspaces[0].name, "alice");
    }

    #[test]
    fn test_authenticate() {
        let db = test_db();
        let (_user, key) = db.create_user("alice", false).unwrap();

        let authed = db.authenticate(&key).unwrap();
        assert!(authed.is_some());
        assert_eq!(authed.unwrap().name, "alice");

        let bad = db.authenticate("b0_invalid").unwrap();
        assert!(bad.is_none());
    }

    #[test]
    fn test_workspace_membership() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();
        let (bob, _) = db.create_user("bob", false).unwrap();

        // Alice creates shared workspace
        db.create_workspace("frontend", &alice.id).unwrap();

        // Add Bob
        db.add_workspace_member("frontend", &bob.id).unwrap();

        // Both are members
        assert!(db.is_workspace_member("frontend", &alice.id).unwrap());
        assert!(db.is_workspace_member("frontend", &bob.id).unwrap());

        // Alice has personal + frontend
        let alice_workspaces = db.list_workspaces_for_user(&alice.id).unwrap();
        assert_eq!(alice_workspaces.len(), 2);

        // Bob has personal + frontend
        let bob_workspaces = db.list_workspaces_for_user(&bob.id).unwrap();
        assert_eq!(bob_workspaces.len(), 2);
    }

    #[test]
    fn test_agent_in_workspace() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();

        db.register_agent("alice", "reviewer", "Code reviewer", "Review code.", "local", "auto", &alice.id, "background", None, None)
            .unwrap();

        let agents = db.list_agents("alice").unwrap();
        assert_eq!(agents.len(), 1);
        assert_eq!(agents[0].name, "reviewer");

        // Not visible in other workspaces
        let (bob, _) = db.create_user("bob", false).unwrap();
        let agents = db.list_agents("bob").unwrap();
        assert_eq!(agents.len(), 0);

        // But visible in shared workspace
        db.create_workspace("team", &alice.id).unwrap();
        db.add_workspace_member("team", &bob.id).unwrap();
        db.register_agent("team", "shared-reviewer", "Shared reviewer", "Review.", "local", "auto", &alice.id, "background", None, None)
            .unwrap();

        let team_agents = db.list_agents("team").unwrap();
        assert_eq!(team_agents.len(), 1);
        assert_eq!(team_agents[0].name, "shared-reviewer");
    }

    #[test]
    fn test_machine_ownership() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();
        let (bob, _) = db.create_user("bob", false).unwrap();

        db.register_machine("alice-gpu", &alice.id).unwrap();

        let owner = db.get_machine_owner("alice-gpu").unwrap();
        assert_eq!(owner, Some(alice.id.clone()));

        // Bob is not the owner
        let owner = db.get_machine_owner("alice-gpu").unwrap();
        assert_ne!(owner, Some(bob.id));
    }

    #[test]
    fn test_agent_ownership_permission() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();
        let (bob, _) = db.create_user("bob", false).unwrap();

        db.create_workspace("team", &alice.id).unwrap();
        db.add_workspace_member("team", &bob.id).unwrap();

        db.register_agent("team", "reviewer", "Reviewer", "Review.", "local", "auto", &alice.id, "background", None, None)
            .unwrap();

        // Bob cannot remove Alice's agent
        let result = db.remove_agent("team", "reviewer", &bob.id);
        assert!(result.is_err());

        // Alice can
        db.remove_agent("team", "reviewer", &alice.id).unwrap();
    }

    #[test]
    fn test_inbox_roundtrip() {
        let db = test_db();
        let (_alice, _) = db.create_user("alice", false).unwrap();

        let msg = db
            .send_inbox_message(
                "alice",
                "t1",
                "sender",
                "receiver",
                "request",
                Some(&serde_json::json!("hello")),
            )
            .unwrap();
        assert_eq!(msg.msg_type, "request");

        let messages = db
            .get_inbox_messages("alice", "receiver", Some("unread"), None)
            .unwrap();
        assert_eq!(messages.len(), 1);

        db.ack_inbox_message("alice", &msg.id).unwrap();
        let messages = db
            .get_inbox_messages("alice", "receiver", Some("unread"), None)
            .unwrap();
        assert_eq!(messages.len(), 0);
    }

    #[test]
    fn test_task_crud() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();

        let task = db
            .create_task(
                "alice",
                "Review PR #42",
                "task-abc",
                "thread-abc",
                None,
                &alice.id,
            )
            .unwrap();
        assert!(task.id.starts_with("task-"));
        assert_eq!(task.status, "running");
        assert_eq!(task.title, "Review PR #42");

        let fetched = db.get_task("alice", &task.id).unwrap();
        assert!(fetched.is_some());
        assert_eq!(fetched.unwrap().title, "Review PR #42");

        let tasks = db.list_tasks("alice").unwrap();
        assert_eq!(tasks.len(), 1);

        db.update_task_status("alice", &task.id, "done", Some("All good"))
            .unwrap();
        let updated = db.get_task("alice", &task.id).unwrap().unwrap();
        assert_eq!(updated.status, "done");
        assert_eq!(updated.result.as_deref(), Some("All good"));
    }

    #[test]
    fn test_task_subtasks() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();

        let parent = db
            .create_task(
                "alice",
                "SEO project",
                "task-seo",
                "thread-1",
                None,
                &alice.id,
            )
            .unwrap();
        let _sub1 = db
            .create_task(
                "alice",
                "Research keywords",
                "worker-1",
                "thread-2",
                Some(&parent.id),
                &alice.id,
            )
            .unwrap();
        let _sub2 = db
            .create_task(
                "alice",
                "Write blog",
                "worker-2",
                "thread-3",
                Some(&parent.id),
                &alice.id,
            )
            .unwrap();

        let subs = db.get_subtasks("alice", &parent.id).unwrap();
        assert_eq!(subs.len(), 2);
    }

    #[test]
    fn test_task_by_thread() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();

        let task = db
            .create_task(
                "alice",
                "Test task",
                "task-xyz",
                "thread-xyz",
                None,
                &alice.id,
            )
            .unwrap();
        let found = db.get_task_by_thread("alice", "thread-xyz").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, task.id);

        let not_found = db.get_task_by_thread("alice", "thread-nope").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_workflow_crud() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();
        db.register_agent(
            "alice",
            "researcher",
            "",
            "Research.",
            "local",
            "auto",
            &alice.id,
            "normal",
        )
        .unwrap();

        let nodes = vec![
            WorkflowNodeDraft {
                id: "node-start".to_string(),
                kind: "start".to_string(),
                title: "Start".to_string(),
                prompt: "".to_string(),
                agent_name: None,
                position_x: 0.0,
                position_y: 0.0,
            },
            WorkflowNodeDraft {
                id: "node-agent".to_string(),
                kind: "agent".to_string(),
                title: "Research".to_string(),
                prompt: "Research the topic".to_string(),
                agent_name: Some("researcher".to_string()),
                position_x: 180.0,
                position_y: 0.0,
            },
            WorkflowNodeDraft {
                id: "node-end".to_string(),
                kind: "end".to_string(),
                title: "End".to_string(),
                prompt: "".to_string(),
                agent_name: None,
                position_x: 360.0,
                position_y: 0.0,
            },
        ];
        let edges = vec![
            WorkflowEdgeDraft {
                id: "edge-1".to_string(),
                source_node_id: "node-start".to_string(),
                target_node_id: "node-agent".to_string(),
            },
            WorkflowEdgeDraft {
                id: "edge-2".to_string(),
                source_node_id: "node-agent".to_string(),
                target_node_id: "node-end".to_string(),
            },
        ];

        let workflow = db
            .create_workflow(
                "alice",
                "Research flow",
                "Draft",
                "draft",
                &alice.id,
                &nodes,
                &edges,
            )
            .unwrap();
        assert!(workflow.id.starts_with("wf-"));

        let list = db.list_workflows("alice").unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].node_count, 3);
        assert_eq!(list[0].agent_count, 1);

        let def = db
            .get_workflow_definition("alice", &workflow.id)
            .unwrap()
            .unwrap();
        assert_eq!(def.workflow.name, "Research flow");
        assert_eq!(def.nodes.len(), 3);
        assert_eq!(def.edges.len(), 2);

        let updated_nodes = vec![
            WorkflowNodeDraft {
                id: "node-start".to_string(),
                kind: "start".to_string(),
                title: "Start".to_string(),
                prompt: "".to_string(),
                agent_name: None,
                position_x: 0.0,
                position_y: 0.0,
            },
            WorkflowNodeDraft {
                id: "node-agent".to_string(),
                kind: "agent".to_string(),
                title: "Research deeply".to_string(),
                prompt: "Research carefully".to_string(),
                agent_name: Some("researcher".to_string()),
                position_x: 220.0,
                position_y: 20.0,
            },
        ];
        let updated_edges = vec![WorkflowEdgeDraft {
            id: "edge-3".to_string(),
            source_node_id: "node-start".to_string(),
            target_node_id: "node-agent".to_string(),
        }];

        let updated = db
            .update_workflow(
                "alice",
                &workflow.id,
                "Research flow v2",
                "Updated",
                "published",
                &alice.id,
                &updated_nodes,
                &updated_edges,
            )
            .unwrap();
        assert_eq!(updated.status, "published");

        let def = db
            .get_workflow_definition("alice", &workflow.id)
            .unwrap()
            .unwrap();
        assert_eq!(def.workflow.name, "Research flow v2");
        assert_eq!(def.nodes.len(), 2);
        assert_eq!(def.edges.len(), 1);

        db.remove_workflow("alice", &workflow.id, &alice.id)
            .unwrap();
        assert!(db.get_workflow("alice", &workflow.id).unwrap().is_none());
    }

    #[test]
    fn test_workflow_run_creation() {
        let db = test_db();
        let (alice, _) = db.create_user("alice", false).unwrap();
        db.register_agent(
            "alice", "reviewer", "", "Review.", "local", "auto", &alice.id, "normal",
        )
        .unwrap();

        let workflow = db
            .create_workflow(
                "alice",
                "Review flow",
                "",
                "draft",
                &alice.id,
                &[
                    WorkflowNodeDraft {
                        id: "start".to_string(),
                        kind: "start".to_string(),
                        title: "Start".to_string(),
                        prompt: "".to_string(),
                        agent_name: None,
                        position_x: 0.0,
                        position_y: 0.0,
                    },
                    WorkflowNodeDraft {
                        id: "review".to_string(),
                        kind: "agent".to_string(),
                        title: "Review".to_string(),
                        prompt: "Review this".to_string(),
                        agent_name: Some("reviewer".to_string()),
                        position_x: 100.0,
                        position_y: 0.0,
                    },
                ],
                &[WorkflowEdgeDraft {
                    id: "edge".to_string(),
                    source_node_id: "start".to_string(),
                    target_node_id: "review".to_string(),
                }],
            )
            .unwrap();

        let run = db
            .create_workflow_run("alice", &workflow.id, Some("Look at PR #1"), &alice.id)
            .unwrap();
        assert_eq!(run.status, "queued");

        let steps = db.list_workflow_step_runs(&run.id).unwrap();
        assert_eq!(steps.len(), 2);
        let start = steps.iter().find(|step| step.node_id == "start").unwrap();
        let review = steps.iter().find(|step| step.node_id == "review").unwrap();
        assert_eq!(start.status, "done");
        assert_eq!(start.output.as_deref(), Some("Look at PR #1"));
        assert_eq!(review.status, "pending");
    }
}
