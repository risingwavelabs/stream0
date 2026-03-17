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
pub struct Topic {
    pub id: i64,
    pub name: String,
    pub retention_days: i32,
    pub created_at: DateTime<Utc>,
    pub message_count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(skip_serializing)]
    pub topic_id: i64,
    pub offset: i64,
    pub payload: serde_json::Value,
    pub headers: serde_json::Map<String, serde_json::Value>,
    pub timestamp: DateTime<Utc>,
    pub delivery_count: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Agent {
    pub id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InboxMessage {
    pub id: String,
    pub task_id: String,
    #[serde(rename = "from")]
    pub from_agent: String,
    #[serde(rename = "to")]
    pub to_agent: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub content: Option<serde_json::Value>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reply {
    pub correlation_id: String,
    pub payload: serde_json::Value,
    pub headers: serde_json::Map<String, serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

impl Database {
    pub fn new(path: &str) -> Result<Self> {
        let conn = Connection::open(path)
            .with_context(|| format!("failed to open database: {}", path))?;

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
            CREATE TABLE IF NOT EXISTS topics (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT UNIQUE NOT NULL,
                retention_days INTEGER DEFAULT 7,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS messages (
                id TEXT PRIMARY KEY,
                topic_id INTEGER NOT NULL,
                offset INTEGER NOT NULL,
                payload TEXT NOT NULL,
                headers TEXT NOT NULL,
                timestamp TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                delivery_count INTEGER DEFAULT 0,
                FOREIGN KEY (topic_id) REFERENCES topics(id),
                UNIQUE(topic_id, offset)
            );

            CREATE TABLE IF NOT EXISTS consumer_groups (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL,
                topic_id INTEGER NOT NULL,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                FOREIGN KEY (topic_id) REFERENCES topics(id),
                UNIQUE(name, topic_id)
            );

            CREATE TABLE IF NOT EXISTS leases (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                message_id TEXT NOT NULL,
                consumer_group TEXT NOT NULL,
                consumer_id TEXT NOT NULL,
                acquired_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                expires_at TEXT NOT NULL,
                delivery_count INTEGER DEFAULT 1,
                FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE,
                UNIQUE(message_id, consumer_group)
            );

            CREATE TABLE IF NOT EXISTS offsets (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                consumer_group TEXT NOT NULL,
                topic_id INTEGER NOT NULL,
                last_offset INTEGER DEFAULT 0,
                updated_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now')),
                FOREIGN KEY (topic_id) REFERENCES topics(id),
                UNIQUE(consumer_group, topic_id)
            );

            CREATE TABLE IF NOT EXISTS replies (
                correlation_id TEXT PRIMARY KEY,
                payload TEXT NOT NULL,
                headers TEXT NOT NULL,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS agents (
                id TEXT PRIMARY KEY,
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE TABLE IF NOT EXISTS inbox_messages (
                id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                from_agent TEXT NOT NULL,
                to_agent TEXT NOT NULL,
                type TEXT NOT NULL,
                content TEXT,
                status TEXT NOT NULL DEFAULT 'unread',
                created_at TEXT DEFAULT (strftime('%Y-%m-%dT%H:%M:%SZ', 'now'))
            );

            CREATE INDEX IF NOT EXISTS idx_messages_topic_offset ON messages(topic_id, offset);
            CREATE INDEX IF NOT EXISTS idx_leases_expires ON leases(expires_at);
            CREATE INDEX IF NOT EXISTS idx_leases_group ON leases(consumer_group);
            CREATE INDEX IF NOT EXISTS idx_replies_created ON replies(created_at);
            CREATE INDEX IF NOT EXISTS idx_inbox_to_status ON inbox_messages(to_agent, status);
            CREATE INDEX IF NOT EXISTS idx_inbox_task ON inbox_messages(task_id);
            CREATE INDEX IF NOT EXISTS idx_inbox_to_task ON inbox_messages(to_agent, task_id);
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

    // --- Topics ---

    pub fn create_topic(&self, name: &str, retention_days: i32) -> Result<Topic> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO topics (name, retention_days) VALUES (?1, ?2)",
            params![name, retention_days],
        )?;
        self.get_topic_inner(&conn, name)?
            .ok_or_else(|| anyhow::anyhow!("topic not found after creation"))
    }

    pub fn get_topic(&self, name: &str) -> Result<Option<Topic>> {
        let conn = self.conn.lock().unwrap();
        self.get_topic_inner(&conn, name)
    }

    fn get_topic_inner(&self, conn: &Connection, name: &str) -> Result<Option<Topic>> {
        conn.query_row(
            "SELECT t.id, t.name, t.retention_days, t.created_at, COUNT(m.id) as message_count
             FROM topics t LEFT JOIN messages m ON t.id = m.topic_id
             WHERE t.name = ?1 GROUP BY t.id",
            params![name],
            |row| {
                let ts: String = row.get(3)?;
                Ok(Topic {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    retention_days: row.get(2)?,
                    created_at: Self::parse_ts(&ts),
                    message_count: row.get(4)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn list_topics(&self) -> Result<Vec<Topic>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT t.id, t.name, t.retention_days, t.created_at, COUNT(m.id)
             FROM topics t LEFT JOIN messages m ON t.id = m.topic_id
             GROUP BY t.id ORDER BY t.name",
        )?;
        let topics = stmt
            .query_map([], |row| {
                let ts: String = row.get(3)?;
                Ok(Topic {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    retention_days: row.get(2)?,
                    created_at: Self::parse_ts(&ts),
                    message_count: row.get(4)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(topics)
    }

    // --- Messages ---

    pub fn publish_message(
        &self,
        topic_id: i64,
        payload: &serde_json::Value,
        headers: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<Message> {
        let conn = self.conn.lock().unwrap();
        let msg_id = format!("msg-{}", &Uuid::new_v4().to_string()[..16]);
        let payload_str = serde_json::to_string(payload)?;
        let headers_str = serde_json::to_string(headers)?;

        let tx = conn.unchecked_transaction()?;
        let offset: i64 = tx.query_row(
            "SELECT COALESCE(MAX(offset), 0) + 1 FROM messages WHERE topic_id = ?1",
            params![topic_id],
            |row| row.get(0),
        )?;

        tx.execute(
            "INSERT INTO messages (id, topic_id, offset, payload, headers) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![msg_id, topic_id, offset, payload_str, headers_str],
        )?;
        tx.commit()?;

        let ts: String = conn.query_row(
            "SELECT timestamp FROM messages WHERE id = ?1",
            params![msg_id],
            |row| row.get(0),
        )?;

        Ok(Message {
            id: msg_id,
            topic_id,
            offset,
            payload: payload.clone(),
            headers: headers.clone(),
            timestamp: Self::parse_ts(&ts),
            delivery_count: 0,
        })
    }

    pub fn get_message(&self, message_id: &str) -> Result<Option<Message>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, topic_id, offset, payload, headers, timestamp, delivery_count FROM messages WHERE id = ?1",
            params![message_id],
            |row| {
                let payload_str: String = row.get(3)?;
                let headers_str: String = row.get(4)?;
                let ts: String = row.get(5)?;
                Ok(Message {
                    id: row.get(0)?,
                    topic_id: row.get(1)?,
                    offset: row.get(2)?,
                    payload: serde_json::from_str(&payload_str).unwrap_or_default(),
                    headers: serde_json::from_str(&headers_str).unwrap_or_default(),
                    timestamp: Self::parse_ts(&ts),
                    delivery_count: row.get(6)?,
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn claim_messages(
        &self,
        topic_id: i64,
        consumer_group: &str,
        consumer_id: &str,
        max_messages: i32,
        visibility_timeout_secs: i32,
    ) -> Result<Vec<Message>> {
        let conn = self.conn.lock().unwrap();
        let now = Utc::now();
        let expires_at = now + chrono::Duration::seconds(visibility_timeout_secs as i64);
        let now_str = now.format("%Y-%m-%dT%H:%M:%SZ").to_string();
        let expires_str = expires_at.format("%Y-%m-%dT%H:%M:%SZ").to_string();

        let tx = conn.unchecked_transaction()?;

        let last_offset: i64 = tx
            .query_row(
                "SELECT last_offset FROM offsets WHERE consumer_group = ?1 AND topic_id = ?2",
                params![consumer_group, topic_id],
                |row| row.get(0),
            )
            .unwrap_or(0);

        let messages: Vec<Message> = {
            let mut stmt = tx.prepare(
                "SELECT m.id, m.topic_id, m.offset, m.payload, m.headers, m.timestamp, m.delivery_count
                 FROM messages m
                 WHERE m.topic_id = ?1 AND m.offset > ?2
                 AND NOT EXISTS (
                    SELECT 1 FROM leases l WHERE l.message_id = m.id
                    AND l.consumer_group = ?3 AND l.expires_at > ?4
                 )
                 ORDER BY m.offset LIMIT ?5",
            )?;

            stmt.query_map(
                params![topic_id, last_offset, consumer_group, now_str, max_messages],
                |row| {
                    let payload_str: String = row.get(3)?;
                    let headers_str: String = row.get(4)?;
                    let ts: String = row.get(5)?;
                    Ok(Message {
                        id: row.get(0)?,
                        topic_id: row.get(1)?,
                        offset: row.get(2)?,
                        payload: serde_json::from_str(&payload_str).unwrap_or_default(),
                        headers: serde_json::from_str(&headers_str).unwrap_or_default(),
                        timestamp: Database::parse_ts(&ts),
                        delivery_count: row.get(6)?,
                    })
                },
            )?
            .collect::<std::result::Result<Vec<_>, _>>()?
        };

        for msg in &messages {
            tx.execute(
                "INSERT INTO leases (message_id, consumer_group, consumer_id, expires_at, delivery_count)
                 VALUES (?1, ?2, ?3, ?4, 1)
                 ON CONFLICT(message_id, consumer_group) DO UPDATE SET
                    consumer_id = excluded.consumer_id,
                    expires_at = excluded.expires_at,
                    delivery_count = leases.delivery_count + 1",
                params![msg.id, consumer_group, consumer_id, expires_str],
            )?;
            tx.execute(
                "UPDATE messages SET delivery_count = delivery_count + 1 WHERE id = ?1",
                params![msg.id],
            )?;
        }

        tx.commit()?;

        // Re-read delivery counts
        let mut result = messages;
        for msg in &mut result {
            msg.delivery_count += 1;
        }
        Ok(result)
    }

    pub fn acknowledge_message(&self, message_id: &str, consumer_group: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let tx = conn.unchecked_transaction()?;

        let deleted = tx.execute(
            "DELETE FROM leases WHERE message_id = ?1 AND consumer_group = ?2",
            params![message_id, consumer_group],
        )?;

        if deleted == 0 {
            anyhow::bail!("message not found or not leased by this group");
        }

        let (topic_id, offset): (i64, i64) = tx.query_row(
            "SELECT topic_id, offset FROM messages WHERE id = ?1",
            params![message_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;

        tx.execute(
            "INSERT INTO offsets (consumer_group, topic_id, last_offset)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(consumer_group, topic_id) DO UPDATE SET
                last_offset = MAX(last_offset, excluded.last_offset),
                updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now')",
            params![consumer_group, topic_id, offset],
        )?;

        tx.commit()?;
        Ok(())
    }

    // --- Request-Reply ---

    pub fn insert_reply(
        &self,
        correlation_id: &str,
        payload: &serde_json::Value,
        headers: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let payload_str = serde_json::to_string(payload)?;
        let headers_str = serde_json::to_string(headers)?;
        conn.execute(
            "INSERT OR REPLACE INTO replies (correlation_id, payload, headers) VALUES (?1, ?2, ?3)",
            params![correlation_id, payload_str, headers_str],
        )?;
        Ok(())
    }

    pub fn get_reply(&self, correlation_id: &str) -> Result<Option<Reply>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT correlation_id, payload, headers, created_at FROM replies WHERE correlation_id = ?1",
            params![correlation_id],
            |row| {
                let payload_str: String = row.get(1)?;
                let headers_str: String = row.get(2)?;
                let ts: String = row.get(3)?;
                Ok(Reply {
                    correlation_id: row.get(0)?,
                    payload: serde_json::from_str(&payload_str).unwrap_or_default(),
                    headers: serde_json::from_str(&headers_str).unwrap_or_default(),
                    created_at: Database::parse_ts(&ts),
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn delete_reply(&self, correlation_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM replies WHERE correlation_id = ?1",
            params![correlation_id],
        )?;
        Ok(())
    }

    // --- Agents (Inbox Model) ---

    pub fn register_agent(&self, id: &str) -> Result<Agent> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT OR IGNORE INTO agents (id) VALUES (?1)",
            params![id],
        )?;
        conn.query_row(
            "SELECT id, created_at FROM agents WHERE id = ?1",
            params![id],
            |row| {
                let ts: String = row.get(1)?;
                Ok(Agent {
                    id: row.get(0)?,
                    created_at: Database::parse_ts(&ts),
                })
            },
        )
        .map_err(Into::into)
    }

    pub fn get_agent(&self, id: &str) -> Result<Option<Agent>> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, created_at FROM agents WHERE id = ?1",
            params![id],
            |row| {
                let ts: String = row.get(1)?;
                Ok(Agent {
                    id: row.get(0)?,
                    created_at: Database::parse_ts(&ts),
                })
            },
        )
        .optional()
        .map_err(Into::into)
    }

    pub fn delete_agent(&self, id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let deleted = conn.execute("DELETE FROM agents WHERE id = ?1", params![id])?;
        if deleted == 0 {
            anyhow::bail!("agent not found");
        }
        Ok(())
    }

    pub fn send_inbox_message(
        &self,
        task_id: &str,
        from: &str,
        to: &str,
        msg_type: &str,
        content: Option<&serde_json::Value>,
    ) -> Result<InboxMessage> {
        let conn = self.conn.lock().unwrap();
        let msg_id = format!("imsg-{}", &Uuid::new_v4().to_string()[..16]);
        let content_str = content.map(|c| serde_json::to_string(c).unwrap_or_default());

        conn.execute(
            "INSERT INTO inbox_messages (id, task_id, from_agent, to_agent, type, content) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![msg_id, task_id, from, to, msg_type, content_str],
        )?;

        let ts: String = conn.query_row(
            "SELECT created_at FROM inbox_messages WHERE id = ?1",
            params![msg_id],
            |row| row.get(0),
        )?;

        Ok(InboxMessage {
            id: msg_id,
            task_id: task_id.to_string(),
            from_agent: from.to_string(),
            to_agent: to.to_string(),
            msg_type: msg_type.to_string(),
            content: content.cloned(),
            status: "unread".to_string(),
            created_at: Self::parse_ts(&ts),
        })
    }

    pub fn get_inbox_messages(
        &self,
        agent_id: &str,
        status: Option<&str>,
        task_id: Option<&str>,
    ) -> Result<Vec<InboxMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut query =
            "SELECT id, task_id, from_agent, to_agent, type, content, status, created_at FROM inbox_messages WHERE to_agent = ?1"
                .to_string();
        let mut param_idx = 2;
        let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = vec![Box::new(agent_id.to_string())];

        if let Some(s) = status {
            query += &format!(" AND status = ?{}", param_idx);
            param_values.push(Box::new(s.to_string()));
            param_idx += 1;
        }
        if let Some(t) = task_id {
            query += &format!(" AND task_id = ?{}", param_idx);
            param_values.push(Box::new(t.to_string()));
        }
        query += " ORDER BY created_at ASC";

        let mut stmt = conn.prepare(&query)?;
        let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| p.as_ref()).collect();
        let messages = stmt
            .query_map(params_ref.as_slice(), |row| {
                let content_str: Option<String> = row.get(5)?;
                let ts: String = row.get(7)?;
                Ok(InboxMessage {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    from_agent: row.get(2)?,
                    to_agent: row.get(3)?,
                    msg_type: row.get(4)?,
                    content: content_str
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    status: row.get(6)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(messages)
    }

    pub fn ack_inbox_message(&self, message_id: &str) -> Result<()> {
        let conn = self.conn.lock().unwrap();
        let updated = conn.execute(
            "UPDATE inbox_messages SET status = 'acked' WHERE id = ?1 AND status = 'unread'",
            params![message_id],
        )?;
        if updated == 0 {
            anyhow::bail!("message not found or already acked");
        }
        Ok(())
    }

    pub fn get_task_messages(&self, task_id: &str) -> Result<Vec<InboxMessage>> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "SELECT id, task_id, from_agent, to_agent, type, content, status, created_at
             FROM inbox_messages WHERE task_id = ?1 ORDER BY created_at ASC",
        )?;
        let messages = stmt
            .query_map(params![task_id], |row| {
                let content_str: Option<String> = row.get(5)?;
                let ts: String = row.get(7)?;
                Ok(InboxMessage {
                    id: row.get(0)?,
                    task_id: row.get(1)?,
                    from_agent: row.get(2)?,
                    to_agent: row.get(3)?,
                    msg_type: row.get(4)?,
                    content: content_str
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    status: row.get(6)?,
                    created_at: Database::parse_ts(&ts),
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(messages)
    }
}
