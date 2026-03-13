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

CREATE INDEX IF NOT EXISTS idx_messages_topic_offset ON messages(topic_id, offset);
CREATE INDEX IF NOT EXISTS idx_leases_expires ON leases(expires_at);
CREATE INDEX IF NOT EXISTS idx_leases_group ON leases(consumer_group);
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
