use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use std::sync::Mutex;
use uuid::Uuid;

pub struct Database {
    conn: Mutex<Connection>,
}

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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub machine_id: String,
    pub runtime: String,
    pub status: String,
    pub registered_by: String,
    pub created_at: DateTime<Utc>,
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
                created_by TEXT NOT NULL,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );
            ",
        )?;
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
        let mut stmt = conn.prepare(
            "SELECT id, name, is_admin, created_at FROM users ORDER BY created_at ASC",
        )?;
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
            .query_row(
                "SELECT COUNT(*) FROM users WHERE is_admin = 1",
                [],
                |row| row.get::<_, i64>(0),
            )
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
        agent_id: &str,
        status: Option<&str>,
        thread_id: Option<&str>,
    ) -> Result<Vec<InboxMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut query =
            "SELECT id, thread_id, from_id, to_id, type, content, status, created_at FROM inbox_messages WHERE workspace_name = ?1 AND to_id = ?2"
                .to_string();
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> =
            vec![Box::new(workspace_name.to_string()), Box::new(agent_id.to_string())];
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
        let mut stmt = conn.prepare(
            "SELECT id, owner, status, last_heartbeat FROM machines ORDER BY id ASC",
        )?;
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
        let ts: String = row.get(7)?;
        Ok(Agent {
            name: row.get(0)?,
            description: row.get(1)?,
            instructions: row.get(2)?,
            machine_id: row.get(3)?,
            runtime: row.get(4)?,
            status: row.get(5)?,
            registered_by: row.get(6)?,
            created_at: Database::parse_ts(&ts),
        })
    }

    const AGENT_COLS: &str = "name, description, instructions, machine_id, runtime, status, registered_by, created_at";

    pub fn register_agent(
        &self,
        workspace_name: &str,
        name: &str,
        description: &str,
        instructions: &str,
        machine_id: &str,
        runtime: &str,
        registered_by: &str,
    ) -> Result<Agent> {
        let conn = self.conn.lock().unwrap();

        conn.execute(
            "INSERT INTO agents (workspace_name, name, description, instructions, machine_id, runtime, registered_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![workspace_name, name, description, instructions, machine_id, runtime, registered_by],
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
            created_at: Self::parse_ts(&ts),
        })
    }

    pub fn list_agents(&self, workspace_name: &str) -> Result<Vec<Agent>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            &format!("SELECT {} FROM agents WHERE workspace_name = ?1 ORDER BY created_at ASC", Self::AGENT_COLS),
        )?;
        let agents = stmt
            .query_map(params![workspace_name], Self::parse_agent_row)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(agents)
    }

    pub fn get_agent(&self, workspace_name: &str, name: &str) -> Result<Option<Agent>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            &format!("SELECT {} FROM agents WHERE workspace_name = ?1 AND name = ?2", Self::AGENT_COLS),
            params![workspace_name, name],
            Self::parse_agent_row,
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn remove_agent(
        &self,
        workspace_name: &str,
        name: &str,
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
            "SELECT workspace_name, name, description, instructions, machine_id, runtime, status, registered_by, created_at FROM agents WHERE machine_id = ?1 AND status = 'active'",
        )?;
        let agents = stmt
            .query_map(params![machine_id], |row| {
                let workspace: String = row.get(0)?;
                let ts: String = row.get(8)?;
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
                        created_at: Database::parse_ts(&ts),
                    },
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(agents)
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

    // --- Cron Jobs ---

    pub fn create_cron_job(
        &self,
        workspace_name: &str,
        agent: &str,
        schedule: &str,
        task: &str,
        created_by: &str,
    ) -> Result<CronJob> {
        let conn = self.conn.lock().unwrap();
        let id = format!("cron-{}", &Uuid::new_v4().to_string()[..8]);
        conn.execute(
            "INSERT INTO cron_jobs (id, workspace_name, agent, schedule, task, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![id, workspace_name, agent, schedule, task, created_by],
        )?;
        Ok(CronJob {
            id,
            workspace_name: workspace_name.to_string(),
            agent: agent.to_string(),
            schedule: schedule.to_string(),
            task: task.to_string(),
            enabled: true,
            last_run: None,
            created_by: created_by.to_string(),
            created_at: Utc::now(),
        })
    }

    pub fn list_cron_jobs(&self, workspace_name: &str) -> Result<Vec<CronJob>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workspace_name, agent, schedule, task, enabled, last_run, created_by, created_at
             FROM cron_jobs WHERE workspace_name = ?1 ORDER BY created_at",
        )?;
        let jobs = stmt
            .query_map(params![workspace_name], |row| {
                let last_run_str: Option<String> = row.get(6)?;
                let ts: String = row.get(8)?;
                let enabled: i32 = row.get(5)?;
                Ok(CronJob {
                    id: row.get(0)?,
                    workspace_name: row.get(1)?,
                    agent: row.get(2)?,
                    schedule: row.get(3)?,
                    task: row.get(4)?,
                    enabled: enabled != 0,
                    last_run: last_run_str.map(|s| Database::parse_ts(&s)),
                    created_by: row.get(7)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(jobs)
    }

    pub fn get_all_enabled_cron_jobs(&self) -> Result<Vec<CronJob>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, workspace_name, agent, schedule, task, enabled, last_run, created_by, created_at
             FROM cron_jobs WHERE enabled = 1",
        )?;
        let jobs = stmt
            .query_map([], |row| {
                let last_run_str: Option<String> = row.get(6)?;
                let ts: String = row.get(8)?;
                Ok(CronJob {
                    id: row.get(0)?,
                    workspace_name: row.get(1)?,
                    agent: row.get(2)?,
                    schedule: row.get(3)?,
                    task: row.get(4)?,
                    enabled: true,
                    last_run: last_run_str.map(|s| Database::parse_ts(&s)),
                    created_by: row.get(7)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(jobs)
    }

    pub fn remove_cron_job(&self, workspace_name: &str, cron_id: &str, user_id: &str) -> Result<()> {
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

    pub fn set_cron_enabled(&self, workspace_name: &str, cron_id: &str, enabled: bool) -> Result<()> {
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

        db.register_agent("alice", "reviewer", "Code reviewer", "Review code.", "local", "auto", &alice.id)
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
        db.register_agent("team", "shared-reviewer", "Shared reviewer", "Review.", "local", "auto", &alice.id)
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

        db.register_agent("team", "reviewer", "Reviewer", "Review.", "local", "auto", &alice.id)
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
            .send_inbox_message("alice", "t1", "sender", "receiver", "request", Some(&serde_json::json!("hello")))
            .unwrap();
        assert_eq!(msg.msg_type, "request");

        let messages = db.get_inbox_messages("alice", "receiver", Some("unread"), None).unwrap();
        assert_eq!(messages.len(), 1);

        db.ack_inbox_message("alice", &msg.id).unwrap();
        let messages = db.get_inbox_messages("alice", "receiver", Some("unread"), None).unwrap();
        assert_eq!(messages.len(), 0);
    }
}
