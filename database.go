package main

import (
	"database/sql"
	"encoding/json"
	"fmt"
	"time"

	"github.com/google/uuid"
	_ "github.com/mattn/go-sqlite3"
)

// Message represents a message in a topic
type Message struct {
	ID            string                 `json:"id"`
	TopicID       int64                  `json:"-"`
	Offset        int64                  `json:"offset"`
	Payload       map[string]interface{} `json:"payload"`
	Headers       map[string]string      `json:"headers"`
	Timestamp     time.Time              `json:"timestamp"`
	DeliveryCount int                    `json:"delivery_count"`
}

// Topic represents a message topic
type Topic struct {
	ID            int64     `json:"id"`
	Name          string    `json:"name"`
	RetentionDays int       `json:"retention_days"`
	CreatedAt     time.Time `json:"created_at"`
	MessageCount  int       `json:"message_count"`
}

// Agent represents a registered agent (v2 inbox model)
type Agent struct {
	ID        string    `json:"id"`
	CreatedAt time.Time `json:"created_at"`
}

// InboxMessage represents a message in an agent's inbox (v2)
type InboxMessage struct {
	ID        string                 `json:"id"`
	TaskID    string                 `json:"task_id"`
	From      string                 `json:"from"`
	To        string                 `json:"to"`
	Type      string                 `json:"type"`
	Content   map[string]interface{} `json:"content"`
	Status    string                 `json:"status"`
	CreatedAt time.Time              `json:"created_at"`
}

// Database handles all SQLite operations
type Database struct {
	db *sql.DB
}

// NewDatabase creates and initializes the database
func NewDatabase(path string) (*Database, error) {
	db, err := sql.Open("sqlite3", path)
	if err != nil {
		return nil, err
	}

	// Set connection pool
	db.SetMaxOpenConns(1) // SQLite supports one writer
	db.SetMaxIdleConns(1)

	// Set pragmas after opening
	if _, err := db.Exec("PRAGMA journal_mode=WAL"); err != nil {
		db.Close()
		return nil, fmt.Errorf("failed to set WAL mode: %w", err)
	}
	if _, err := db.Exec("PRAGMA synchronous=NORMAL"); err != nil {
		db.Close()
		return nil, fmt.Errorf("failed to set synchronous mode: %w", err)
	}

	d := &Database{db: db}
	if err := d.initSchema(); err != nil {
		return nil, err
	}

	return d, nil
}

// Close closes the database
func (d *Database) Close() error {
	return d.db.Close()
}

// initSchema creates the database tables
func (d *Database) initSchema() error {
	schema := `
CREATE TABLE IF NOT EXISTS topics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT UNIQUE NOT NULL,
    retention_days INTEGER DEFAULT 7,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS messages (
    id TEXT PRIMARY KEY,
    topic_id INTEGER NOT NULL,
    offset INTEGER NOT NULL,
    payload TEXT NOT NULL,
    headers TEXT NOT NULL,
    timestamp TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    delivery_count INTEGER DEFAULT 0,
    FOREIGN KEY (topic_id) REFERENCES topics(id),
    UNIQUE(topic_id, offset)
);

CREATE TABLE IF NOT EXISTS consumer_groups (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    topic_id INTEGER NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (topic_id) REFERENCES topics(id),
    UNIQUE(name, topic_id)
);

CREATE TABLE IF NOT EXISTS leases (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL,
    consumer_group TEXT NOT NULL,
    consumer_id TEXT NOT NULL,
    acquired_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    expires_at TIMESTAMP NOT NULL,
    delivery_count INTEGER DEFAULT 1,
    FOREIGN KEY (message_id) REFERENCES messages(id) ON DELETE CASCADE,
    UNIQUE(message_id, consumer_group)
);

CREATE TABLE IF NOT EXISTS offsets (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    consumer_group TEXT NOT NULL,
    topic_id INTEGER NOT NULL,
    last_offset INTEGER DEFAULT 0,
    updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (topic_id) REFERENCES topics(id),
    UNIQUE(consumer_group, topic_id)
);

CREATE TABLE IF NOT EXISTS dlq (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    message_id TEXT NOT NULL,
    topic_id INTEGER NOT NULL,
    error TEXT,
    retries INTEGER DEFAULT 0,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (message_id) REFERENCES messages(id),
    FOREIGN KEY (topic_id) REFERENCES topics(id)
);

CREATE TABLE IF NOT EXISTS replies (
    correlation_id TEXT PRIMARY KEY,
    payload TEXT NOT NULL,
    headers TEXT NOT NULL,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_messages_topic_offset ON messages(topic_id, offset);
CREATE INDEX IF NOT EXISTS idx_leases_expires ON leases(expires_at);
CREATE INDEX IF NOT EXISTS idx_leases_group ON leases(consumer_group);
CREATE INDEX IF NOT EXISTS idx_replies_created ON replies(created_at);

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE IF NOT EXISTS inbox_messages (
    id TEXT PRIMARY KEY,
    task_id TEXT NOT NULL,
    from_agent TEXT NOT NULL,
    to_agent TEXT NOT NULL,
    type TEXT NOT NULL,
    content TEXT,
    status TEXT NOT NULL DEFAULT 'unread',
    created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
);

CREATE INDEX IF NOT EXISTS idx_inbox_to_status ON inbox_messages(to_agent, status);
CREATE INDEX IF NOT EXISTS idx_inbox_task ON inbox_messages(task_id);
CREATE INDEX IF NOT EXISTS idx_inbox_to_task ON inbox_messages(to_agent, task_id);
`
	_, err := d.db.Exec(schema)
	return err
}

// CreateTopic creates a new topic
func (d *Database) CreateTopic(name string, retentionDays int) (*Topic, error) {
	_, err := d.db.Exec(
		"INSERT OR IGNORE INTO topics (name, retention_days) VALUES (?, ?)",
		name, retentionDays,
	)
	if err != nil {
		return nil, err
	}

	// Get the topic (may have existed)
	return d.GetTopic(name)
}

// GetTopic gets a topic by name
func (d *Database) GetTopic(name string) (*Topic, error) {
	row := d.db.QueryRow(`
		SELECT t.id, t.name, t.retention_days, t.created_at, COUNT(m.id) as message_count
		FROM topics t
		LEFT JOIN messages m ON t.id = m.topic_id
		WHERE t.name = ?
		GROUP BY t.id
	`, name)

	var topic Topic
	var createdAtStr string
	err := row.Scan(&topic.ID, &topic.Name, &topic.RetentionDays, &createdAtStr, &topic.MessageCount)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}

	topic.CreatedAt, _ = time.Parse("2006-01-02 15:04:05", createdAtStr)
	return &topic, nil
}

// ListTopics lists all topics
func (d *Database) ListTopics() ([]Topic, error) {
	rows, err := d.db.Query(`
		SELECT t.id, t.name, t.retention_days, t.created_at, COUNT(m.id) as message_count
		FROM topics t
		LEFT JOIN messages m ON t.id = m.topic_id
		GROUP BY t.id
		ORDER BY t.name
	`)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var topics []Topic
	for rows.Next() {
		var topic Topic
		var createdAtStr string
		if err := rows.Scan(&topic.ID, &topic.Name, &topic.RetentionDays, &createdAtStr, &topic.MessageCount); err != nil {
			return nil, err
		}
		topic.CreatedAt, _ = time.Parse("2006-01-02 15:04:05", createdAtStr)
		topics = append(topics, topic)
	}

	return topics, nil
}

// PublishMessage publishes a message to a topic
func (d *Database) PublishMessage(topicID int64, payload map[string]interface{}, headers map[string]string) (*Message, error) {
	msgID := fmt.Sprintf("msg-%s", uuid.New().String()[:16])

	payloadJSON, _ := json.Marshal(payload)
	headersJSON, _ := json.Marshal(headers)

	tx, err := d.db.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()

	// Get next offset
	var offset int64
	err = tx.QueryRow(
		"SELECT COALESCE(MAX(offset), 0) + 1 FROM messages WHERE topic_id = ?",
		topicID,
	).Scan(&offset)
	if err != nil {
		return nil, err
	}

	// Insert message
	_, err = tx.Exec(
		"INSERT INTO messages (id, topic_id, offset, payload, headers) VALUES (?, ?, ?, ?, ?)",
		msgID, topicID, offset, payloadJSON, headersJSON,
	)
	if err != nil {
		return nil, err
	}

	if err := tx.Commit(); err != nil {
		return nil, err
	}

	return &Message{
		ID:        msgID,
		TopicID:   topicID,
		Offset:    offset,
		Payload:   payload,
		Headers:   headers,
		Timestamp: time.Now().UTC(),
	}, nil
}

// ClaimMessages claims available messages for a consumer group
func (d *Database) ClaimMessages(topicID int64, consumerGroup, consumerID string, maxMessages int, visibilityTimeoutSeconds int) ([]Message, error) {
	tx, err := d.db.Begin()
	if err != nil {
		return nil, err
	}
	defer tx.Rollback()

	now := time.Now().UTC()
	expiresAt := now.Add(time.Duration(visibilityTimeoutSeconds) * time.Second)

	// Get last acknowledged offset for this consumer group
	var lastOffset int64
	err = tx.QueryRow(
		"SELECT last_offset FROM offsets WHERE consumer_group = ? AND topic_id = ?",
		consumerGroup, topicID,
	).Scan(&lastOffset)
	if err == sql.ErrNoRows {
		lastOffset = 0
	} else if err != nil {
		return nil, err
	}

	// Find available messages
	rows, err := tx.Query(`
		SELECT m.id, m.topic_id, m.offset, m.payload, m.headers, m.timestamp, m.delivery_count
		FROM messages m
		WHERE m.topic_id = ?
		AND m.offset > ?
		AND NOT EXISTS (
			SELECT 1 FROM leases l
			WHERE l.message_id = m.id
			AND l.consumer_group = ?
			AND l.expires_at > ?
		)
		ORDER BY m.offset
		LIMIT ?
	`, topicID, lastOffset, consumerGroup, now, maxMessages)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var messages []Message
	for rows.Next() {
		var msg Message
		var payloadStr, headersStr string
		var timestampStr string

		err := rows.Scan(&msg.ID, &msg.TopicID, &msg.Offset, &payloadStr, &headersStr, &timestampStr, &msg.DeliveryCount)
		if err != nil {
			return nil, err
		}

		json.Unmarshal([]byte(payloadStr), &msg.Payload)
		json.Unmarshal([]byte(headersStr), &msg.Headers)
		msg.Timestamp, _ = time.Parse("2006-01-02 15:04:05", timestampStr)

		// Create or update lease
		_, err = tx.Exec(`
			INSERT INTO leases (message_id, consumer_group, consumer_id, expires_at, delivery_count)
			VALUES (?, ?, ?, ?, 1)
			ON CONFLICT(message_id, consumer_group) DO UPDATE SET
				consumer_id = excluded.consumer_id,
				expires_at = excluded.expires_at,
				delivery_count = leases.delivery_count + 1
		`, msg.ID, consumerGroup, consumerID, expiresAt)
		if err != nil {
			return nil, err
		}

		// Update message delivery count
		_, err = tx.Exec("UPDATE messages SET delivery_count = delivery_count + 1 WHERE id = ?", msg.ID)
		if err != nil {
			return nil, err
		}

		msg.DeliveryCount++
		messages = append(messages, msg)
	}

	if err := tx.Commit(); err != nil {
		return nil, err
	}

	return messages, nil
}

// Reply represents a reply to a request
type Reply struct {
	CorrelationID string                 `json:"correlation_id"`
	Payload       map[string]interface{} `json:"payload"`
	Headers       map[string]string      `json:"headers"`
	CreatedAt     time.Time              `json:"created_at"`
}

// GetMessage retrieves a message by ID
func (d *Database) GetMessage(messageID string) (*Message, error) {
	row := d.db.QueryRow(`
		SELECT id, topic_id, offset, payload, headers, timestamp, delivery_count
		FROM messages WHERE id = ?
	`, messageID)

	var msg Message
	var payloadStr, headersStr, timestampStr string
	err := row.Scan(&msg.ID, &msg.TopicID, &msg.Offset, &payloadStr, &headersStr, &timestampStr, &msg.DeliveryCount)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}

	json.Unmarshal([]byte(payloadStr), &msg.Payload)
	json.Unmarshal([]byte(headersStr), &msg.Headers)
	msg.Timestamp, _ = time.Parse("2006-01-02 15:04:05", timestampStr)
	return &msg, nil
}

// InsertReply stores a reply for a correlation ID
func (d *Database) InsertReply(correlationID string, payload map[string]interface{}, headers map[string]string) error {
	payloadJSON, _ := json.Marshal(payload)
	headersJSON, _ := json.Marshal(headers)
	_, err := d.db.Exec(
		"INSERT OR REPLACE INTO replies (correlation_id, payload, headers) VALUES (?, ?, ?)",
		correlationID, payloadJSON, headersJSON,
	)
	return err
}

// GetReply retrieves a reply by correlation ID
func (d *Database) GetReply(correlationID string) (*Reply, error) {
	row := d.db.QueryRow(
		"SELECT correlation_id, payload, headers, created_at FROM replies WHERE correlation_id = ?",
		correlationID,
	)

	var reply Reply
	var payloadStr, headersStr, createdAtStr string
	err := row.Scan(&reply.CorrelationID, &payloadStr, &headersStr, &createdAtStr)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}

	json.Unmarshal([]byte(payloadStr), &reply.Payload)
	json.Unmarshal([]byte(headersStr), &reply.Headers)
	reply.CreatedAt, _ = time.Parse("2006-01-02 15:04:05", createdAtStr)
	return &reply, nil
}

// DeleteReply removes a reply by correlation ID
func (d *Database) DeleteReply(correlationID string) error {
	_, err := d.db.Exec("DELETE FROM replies WHERE correlation_id = ?", correlationID)
	return err
}

// --- v2 Inbox Model Methods ---

// RegisterAgent registers a new agent
func (d *Database) RegisterAgent(id string) (*Agent, error) {
	_, err := d.db.Exec(
		"INSERT OR IGNORE INTO agents (id) VALUES (?)", id,
	)
	if err != nil {
		return nil, err
	}
	return d.GetAgent(id)
}

// GetAgent gets an agent by ID
func (d *Database) GetAgent(id string) (*Agent, error) {
	row := d.db.QueryRow("SELECT id, created_at FROM agents WHERE id = ?", id)
	var agent Agent
	var createdAtStr string
	err := row.Scan(&agent.ID, &createdAtStr)
	if err == sql.ErrNoRows {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	agent.CreatedAt, _ = time.Parse("2006-01-02 15:04:05", createdAtStr)
	return &agent, nil
}

// DeleteAgent deletes an agent
func (d *Database) DeleteAgent(id string) error {
	result, err := d.db.Exec("DELETE FROM agents WHERE id = ?", id)
	if err != nil {
		return err
	}
	rows, _ := result.RowsAffected()
	if rows == 0 {
		return fmt.Errorf("agent not found")
	}
	return nil
}

// SendInboxMessage sends a message to an agent's inbox
func (d *Database) SendInboxMessage(taskID, from, to, msgType string, content map[string]interface{}) (*InboxMessage, error) {
	msgID := fmt.Sprintf("imsg-%s", uuid.New().String()[:16])
	var contentJSON []byte
	if content != nil {
		contentJSON, _ = json.Marshal(content)
	}

	_, err := d.db.Exec(
		"INSERT INTO inbox_messages (id, task_id, from_agent, to_agent, type, content) VALUES (?, ?, ?, ?, ?, ?)",
		msgID, taskID, from, to, msgType, string(contentJSON),
	)
	if err != nil {
		return nil, err
	}

	return &InboxMessage{
		ID:        msgID,
		TaskID:    taskID,
		From:      from,
		To:        to,
		Type:      msgType,
		Content:   content,
		Status:    "unread",
		CreatedAt: time.Now().UTC(),
	}, nil
}

// GetInboxMessages retrieves messages from an agent's inbox
func (d *Database) GetInboxMessages(agentID, status, taskID string) ([]InboxMessage, error) {
	query := "SELECT id, task_id, from_agent, to_agent, type, content, status, created_at FROM inbox_messages WHERE to_agent = ?"
	args := []interface{}{agentID}

	if status != "" {
		query += " AND status = ?"
		args = append(args, status)
	}
	if taskID != "" {
		query += " AND task_id = ?"
		args = append(args, taskID)
	}

	query += " ORDER BY created_at ASC"

	rows, err := d.db.Query(query, args...)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var messages []InboxMessage
	for rows.Next() {
		var msg InboxMessage
		var contentStr sql.NullString
		var createdAtStr string
		if err := rows.Scan(&msg.ID, &msg.TaskID, &msg.From, &msg.To, &msg.Type, &contentStr, &msg.Status, &createdAtStr); err != nil {
			return nil, err
		}
		if contentStr.Valid && contentStr.String != "" {
			json.Unmarshal([]byte(contentStr.String), &msg.Content)
		}
		msg.CreatedAt, _ = time.Parse("2006-01-02 15:04:05", createdAtStr)
		messages = append(messages, msg)
	}
	return messages, nil
}

// AckInboxMessage marks an inbox message as acked
func (d *Database) AckInboxMessage(messageID string) error {
	result, err := d.db.Exec(
		"UPDATE inbox_messages SET status = 'acked' WHERE id = ? AND status = 'unread'",
		messageID,
	)
	if err != nil {
		return err
	}
	rows, _ := result.RowsAffected()
	if rows == 0 {
		return fmt.Errorf("message not found or already acked")
	}
	return nil
}

// GetTaskMessages retrieves all messages for a task (conversation history)
func (d *Database) GetTaskMessages(taskID string) ([]InboxMessage, error) {
	rows, err := d.db.Query(
		"SELECT id, task_id, from_agent, to_agent, type, content, status, created_at FROM inbox_messages WHERE task_id = ? ORDER BY created_at ASC",
		taskID,
	)
	if err != nil {
		return nil, err
	}
	defer rows.Close()

	var messages []InboxMessage
	for rows.Next() {
		var msg InboxMessage
		var contentStr sql.NullString
		var createdAtStr string
		if err := rows.Scan(&msg.ID, &msg.TaskID, &msg.From, &msg.To, &msg.Type, &contentStr, &msg.Status, &createdAtStr); err != nil {
			return nil, err
		}
		if contentStr.Valid && contentStr.String != "" {
			json.Unmarshal([]byte(contentStr.String), &msg.Content)
		}
		msg.CreatedAt, _ = time.Parse("2006-01-02 15:04:05", createdAtStr)
		messages = append(messages, msg)
	}
	return messages, nil
}

// AcknowledgeMessage acknowledges a message
func (d *Database) AcknowledgeMessage(messageID, consumerGroup string) error {
	tx, err := d.db.Begin()
	if err != nil {
		return err
	}
	defer tx.Rollback()

	// Delete lease
	result, err := tx.Exec("DELETE FROM leases WHERE message_id = ? AND consumer_group = ?", messageID, consumerGroup)
	if err != nil {
		return err
	}

	rowsAffected, _ := result.RowsAffected()
	if rowsAffected == 0 {
		return fmt.Errorf("message not found or not leased by this group")
	}

	// Get topic_id and offset
	var topicID, offset int64
	err = tx.QueryRow("SELECT topic_id, offset FROM messages WHERE id = ?", messageID).Scan(&topicID, &offset)
	if err != nil {
		return err
	}

	// Update consumer offset
	_, err = tx.Exec(`
		INSERT INTO offsets (consumer_group, topic_id, last_offset)
		VALUES (?, ?, ?)
		ON CONFLICT(consumer_group, topic_id) DO UPDATE SET
			last_offset = MAX(last_offset, excluded.last_offset),
			updated_at = CURRENT_TIMESTAMP
	`, consumerGroup, topicID, offset)
	if err != nil {
		return err
	}

	return tx.Commit()
}
