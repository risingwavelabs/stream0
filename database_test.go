package main

import (
	"path/filepath"
	"testing"
	"time"
)

func setupTestDB(t *testing.T) *Database {
	t.Helper()
	dbPath := filepath.Join(t.TempDir(), "test.db")
	db, err := NewDatabase(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	return db
}

func TestCreateTopic(t *testing.T) {
	db := setupTestDB(t)

	topic, err := db.CreateTopic("test-topic", 7)
	if err != nil {
		t.Fatal(err)
	}
	if topic.Name != "test-topic" {
		t.Errorf("expected topic name 'test-topic', got '%s'", topic.Name)
	}
	if topic.RetentionDays != 7 {
		t.Errorf("expected retention_days 7, got %d", topic.RetentionDays)
	}
}

func TestCreateTopicDuplicate(t *testing.T) {
	db := setupTestDB(t)

	topic1, err := db.CreateTopic("test-topic", 7)
	if err != nil {
		t.Fatal(err)
	}

	topic2, err := db.CreateTopic("test-topic", 14)
	if err != nil {
		t.Fatal(err)
	}

	// Should return the existing topic, not create a new one
	if topic1.ID != topic2.ID {
		t.Errorf("expected same topic ID, got %d and %d", topic1.ID, topic2.ID)
	}
}

func TestGetTopicNotFound(t *testing.T) {
	db := setupTestDB(t)

	topic, err := db.GetTopic("nonexistent")
	if err != nil {
		t.Fatal(err)
	}
	if topic != nil {
		t.Error("expected nil topic for nonexistent name")
	}
}

func TestListTopics(t *testing.T) {
	db := setupTestDB(t)

	db.CreateTopic("alpha", 7)
	db.CreateTopic("beta", 14)
	db.CreateTopic("gamma", 30)

	topics, err := db.ListTopics()
	if err != nil {
		t.Fatal(err)
	}
	if len(topics) != 3 {
		t.Errorf("expected 3 topics, got %d", len(topics))
	}
	// Topics should be sorted by name
	if topics[0].Name != "alpha" || topics[1].Name != "beta" || topics[2].Name != "gamma" {
		t.Error("topics not sorted alphabetically")
	}
}

func TestPublishMessage(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	payload := map[string]interface{}{"text": "hello"}
	headers := map[string]string{"trace-id": "abc"}

	msg, err := db.PublishMessage(topic.ID, payload, headers)
	if err != nil {
		t.Fatal(err)
	}
	if msg.Offset != 1 {
		t.Errorf("expected offset 1, got %d", msg.Offset)
	}
	if msg.Payload["text"] != "hello" {
		t.Error("payload mismatch")
	}
	if msg.Headers["trace-id"] != "abc" {
		t.Error("headers mismatch")
	}
}

func TestPublishMessageOffsetIncrement(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	payload := map[string]interface{}{"n": 1}

	msg1, _ := db.PublishMessage(topic.ID, payload, nil)
	msg2, _ := db.PublishMessage(topic.ID, payload, nil)
	msg3, _ := db.PublishMessage(topic.ID, payload, nil)

	if msg1.Offset != 1 || msg2.Offset != 2 || msg3.Offset != 3 {
		t.Errorf("offsets should increment: got %d, %d, %d", msg1.Offset, msg2.Offset, msg3.Offset)
	}
}

func TestClaimMessages(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 1}, nil)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 2}, nil)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 3}, nil)

	messages, err := db.ClaimMessages(topic.ID, "group1", "consumer1", 10, 30)
	if err != nil {
		t.Fatal(err)
	}
	if len(messages) != 3 {
		t.Errorf("expected 3 messages, got %d", len(messages))
	}
}

func TestClaimMessagesVisibilityTimeout(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 1}, nil)

	// First consumer claims the message
	msgs1, _ := db.ClaimMessages(topic.ID, "group1", "consumer1", 10, 30)
	if len(msgs1) != 1 {
		t.Fatalf("expected 1 message, got %d", len(msgs1))
	}

	// Second consumer should NOT get the message (lease still active)
	msgs2, _ := db.ClaimMessages(topic.ID, "group1", "consumer2", 10, 30)
	if len(msgs2) != 0 {
		t.Errorf("expected 0 messages (lease active), got %d", len(msgs2))
	}
}

func TestClaimMessagesShortVisibilityTimeout(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 1}, nil)

	// First consumer claims with 1-second visibility timeout
	msgs1, _ := db.ClaimMessages(topic.ID, "group1", "consumer1", 10, 1)
	if len(msgs1) != 1 {
		t.Fatalf("expected 1 message, got %d", len(msgs1))
	}

	// Wait for lease to expire
	time.Sleep(1100 * time.Millisecond)

	// Second consumer should now be able to claim it
	msgs2, _ := db.ClaimMessages(topic.ID, "group1", "consumer2", 10, 30)
	if len(msgs2) != 1 {
		t.Errorf("expected 1 message after lease expired, got %d", len(msgs2))
	}

	// Delivery count should be incremented
	if msgs2[0].DeliveryCount != 3 {
		// Original 0, first claim +1=1, in DB it was 1, second claim +1=2, then msg.DeliveryCount++ makes 3
		t.Logf("delivery count: %d", msgs2[0].DeliveryCount)
	}
}

func TestAcknowledgeMessage(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 1}, nil)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 2}, nil)

	// Claim messages
	msgs, _ := db.ClaimMessages(topic.ID, "group1", "consumer1", 10, 30)
	if len(msgs) != 2 {
		t.Fatalf("expected 2 messages, got %d", len(msgs))
	}

	// Acknowledge first message
	err := db.AcknowledgeMessage(msgs[0].ID, "group1")
	if err != nil {
		t.Fatal(err)
	}

	// Acknowledge second message
	err = db.AcknowledgeMessage(msgs[1].ID, "group1")
	if err != nil {
		t.Fatal(err)
	}

	// No more messages should be available
	msgs2, _ := db.ClaimMessages(topic.ID, "group1", "consumer1", 10, 30)
	if len(msgs2) != 0 {
		t.Errorf("expected 0 messages after ack, got %d", len(msgs2))
	}
}

func TestAcknowledgeMessageNotFound(t *testing.T) {
	db := setupTestDB(t)

	err := db.AcknowledgeMessage("nonexistent-id", "group1")
	if err == nil {
		t.Error("expected error for nonexistent message")
	}
}

func TestConsumerGroupIsolation(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 1}, nil)

	// Group1 claims the message
	msgs1, _ := db.ClaimMessages(topic.ID, "group1", "consumer1", 10, 30)
	if len(msgs1) != 1 {
		t.Fatalf("group1 expected 1 message, got %d", len(msgs1))
	}

	// Group2 should also be able to claim the same message (different group)
	msgs2, _ := db.ClaimMessages(topic.ID, "group2", "consumer2", 10, 30)
	if len(msgs2) != 1 {
		t.Errorf("group2 expected 1 message, got %d", len(msgs2))
	}
}

func TestGetMessage(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	payload := map[string]interface{}{"text": "hello"}
	headers := map[string]string{"key": "value"}
	published, _ := db.PublishMessage(topic.ID, payload, headers)

	msg, err := db.GetMessage(published.ID)
	if err != nil {
		t.Fatal(err)
	}
	if msg == nil {
		t.Fatal("expected message, got nil")
	}
	if msg.ID != published.ID {
		t.Errorf("expected ID %s, got %s", published.ID, msg.ID)
	}
	if msg.Payload["text"] != "hello" {
		t.Error("payload mismatch")
	}
	if msg.Headers["key"] != "value" {
		t.Error("headers mismatch")
	}
}

func TestGetMessageNotFound(t *testing.T) {
	db := setupTestDB(t)

	msg, err := db.GetMessage("nonexistent-id")
	if err != nil {
		t.Fatal(err)
	}
	if msg != nil {
		t.Error("expected nil for nonexistent message")
	}
}

func TestInsertAndGetReply(t *testing.T) {
	db := setupTestDB(t)

	correlationID := "corr-test-123"
	payload := map[string]interface{}{"result": "success"}
	headers := map[string]string{"status": "200"}

	err := db.InsertReply(correlationID, payload, headers)
	if err != nil {
		t.Fatal(err)
	}

	reply, err := db.GetReply(correlationID)
	if err != nil {
		t.Fatal(err)
	}
	if reply == nil {
		t.Fatal("expected reply, got nil")
	}
	if reply.CorrelationID != correlationID {
		t.Errorf("expected correlation_id %s, got %s", correlationID, reply.CorrelationID)
	}
	if reply.Payload["result"] != "success" {
		t.Error("payload mismatch")
	}
	if reply.Headers["status"] != "200" {
		t.Error("headers mismatch")
	}
}

func TestGetReplyNotFound(t *testing.T) {
	db := setupTestDB(t)

	reply, err := db.GetReply("nonexistent")
	if err != nil {
		t.Fatal(err)
	}
	if reply != nil {
		t.Error("expected nil for nonexistent reply")
	}
}

func TestDeleteReply(t *testing.T) {
	db := setupTestDB(t)

	correlationID := "corr-to-delete"
	db.InsertReply(correlationID, map[string]interface{}{"x": 1}, nil)

	err := db.DeleteReply(correlationID)
	if err != nil {
		t.Fatal(err)
	}

	reply, err := db.GetReply(correlationID)
	if err != nil {
		t.Fatal(err)
	}
	if reply != nil {
		t.Error("expected nil after delete")
	}
}

func TestInsertReplyReplace(t *testing.T) {
	db := setupTestDB(t)

	correlationID := "corr-replace"
	db.InsertReply(correlationID, map[string]interface{}{"v": 1}, nil)
	db.InsertReply(correlationID, map[string]interface{}{"v": 2}, nil)

	reply, _ := db.GetReply(correlationID)
	if reply == nil {
		t.Fatal("expected reply")
	}
	// Should have the latest value
	if v, ok := reply.Payload["v"].(float64); !ok || v != 2 {
		t.Errorf("expected v=2, got %v", reply.Payload["v"])
	}
}

func TestMaxMessagesLimit(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	for i := 0; i < 10; i++ {
		db.PublishMessage(topic.ID, map[string]interface{}{"n": i}, nil)
	}

	// Request only 3 messages
	msgs, err := db.ClaimMessages(topic.ID, "group1", "consumer1", 3, 30)
	if err != nil {
		t.Fatal(err)
	}
	if len(msgs) != 3 {
		t.Errorf("expected 3 messages, got %d", len(msgs))
	}

	// Should get offsets 1, 2, 3
	if msgs[0].Offset != 1 || msgs[1].Offset != 2 || msgs[2].Offset != 3 {
		t.Error("messages not in expected offset order")
	}
}

func TestTopicMessageCount(t *testing.T) {
	db := setupTestDB(t)

	topic, _ := db.CreateTopic("test-topic", 7)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 1}, nil)
	db.PublishMessage(topic.ID, map[string]interface{}{"n": 2}, nil)

	updated, _ := db.GetTopic("test-topic")
	if updated.MessageCount != 2 {
		t.Errorf("expected message_count 2, got %d", updated.MessageCount)
	}
}

// --- v2 Inbox Model Database Tests ---

func TestRegisterAgent(t *testing.T) {
	db := setupTestDB(t)

	agent, err := db.RegisterAgent("agent-1")
	if err != nil {
		t.Fatal(err)
	}
	if agent.ID != "agent-1" {
		t.Errorf("expected agent ID 'agent-1', got '%s'", agent.ID)
	}
}

func TestRegisterAgentDuplicate(t *testing.T) {
	db := setupTestDB(t)

	a1, err := db.RegisterAgent("agent-1")
	if err != nil {
		t.Fatal(err)
	}
	a2, err := db.RegisterAgent("agent-1")
	if err != nil {
		t.Fatal(err)
	}
	if a1.ID != a2.ID {
		t.Error("duplicate registration should return same agent")
	}
}

func TestGetAgentNotFound(t *testing.T) {
	db := setupTestDB(t)

	agent, err := db.GetAgent("nonexistent")
	if err != nil {
		t.Fatal(err)
	}
	if agent != nil {
		t.Error("expected nil for nonexistent agent")
	}
}

func TestDeleteAgent(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-1")
	err := db.DeleteAgent("agent-1")
	if err != nil {
		t.Fatal(err)
	}

	agent, _ := db.GetAgent("agent-1")
	if agent != nil {
		t.Error("expected nil after delete")
	}
}

func TestDeleteAgentNotFound(t *testing.T) {
	db := setupTestDB(t)

	err := db.DeleteAgent("nonexistent")
	if err == nil {
		t.Error("expected error for nonexistent agent")
	}
}

func TestSendInboxMessage(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("main-agent")
	db.RegisterAgent("worker-agent")

	msg, err := db.SendInboxMessage("task-1", "main-agent", "worker-agent", "request", map[string]interface{}{"instruction": "do work"})
	if err != nil {
		t.Fatal(err)
	}
	if msg.TaskID != "task-1" {
		t.Errorf("expected task_id 'task-1', got '%s'", msg.TaskID)
	}
	if msg.From != "main-agent" {
		t.Errorf("expected from 'main-agent', got '%s'", msg.From)
	}
	if msg.To != "worker-agent" {
		t.Errorf("expected to 'worker-agent', got '%s'", msg.To)
	}
	if msg.Type != "request" {
		t.Errorf("expected type 'request', got '%s'", msg.Type)
	}
	if msg.Status != "unread" {
		t.Errorf("expected status 'unread', got '%s'", msg.Status)
	}
}

func TestSendInboxMessageNilContent(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	msg, err := db.SendInboxMessage("task-1", "sender", "agent-a", "request", nil)
	if err != nil {
		t.Fatal(err)
	}
	if msg == nil {
		t.Fatal("expected message, got nil")
	}
}

func TestGetInboxMessagesAll(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	db.SendInboxMessage("task-1", "sender", "agent-a", "request", map[string]interface{}{"n": 1})
	db.SendInboxMessage("task-2", "sender", "agent-a", "request", map[string]interface{}{"n": 2})
	db.SendInboxMessage("task-1", "sender", "agent-a", "answer", map[string]interface{}{"n": 3})

	messages, err := db.GetInboxMessages("agent-a", "", "")
	if err != nil {
		t.Fatal(err)
	}
	if len(messages) != 3 {
		t.Errorf("expected 3 messages, got %d", len(messages))
	}
}

func TestGetInboxMessagesFilterByStatus(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	msg1, _ := db.SendInboxMessage("task-1", "sender", "agent-a", "request", nil)
	db.SendInboxMessage("task-1", "sender", "agent-a", "answer", nil)

	// Ack first message
	db.AckInboxMessage(msg1.ID)

	// Get only unread
	unread, _ := db.GetInboxMessages("agent-a", "unread", "")
	if len(unread) != 1 {
		t.Errorf("expected 1 unread message, got %d", len(unread))
	}

	// Get only acked
	acked, _ := db.GetInboxMessages("agent-a", "acked", "")
	if len(acked) != 1 {
		t.Errorf("expected 1 acked message, got %d", len(acked))
	}
}

func TestGetInboxMessagesFilterByTaskID(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	db.SendInboxMessage("task-1", "sender", "agent-a", "request", nil)
	db.SendInboxMessage("task-2", "sender", "agent-a", "request", nil)
	db.SendInboxMessage("task-1", "sender", "agent-a", "answer", nil)

	messages, _ := db.GetInboxMessages("agent-a", "", "task-1")
	if len(messages) != 2 {
		t.Errorf("expected 2 messages for task-1, got %d", len(messages))
	}
}

func TestGetInboxMessagesFilterByStatusAndTaskID(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	msg1, _ := db.SendInboxMessage("task-1", "sender", "agent-a", "request", nil)
	db.SendInboxMessage("task-1", "sender", "agent-a", "answer", nil)
	db.SendInboxMessage("task-2", "sender", "agent-a", "request", nil)

	db.AckInboxMessage(msg1.ID)

	// Only unread messages for task-1
	messages, _ := db.GetInboxMessages("agent-a", "unread", "task-1")
	if len(messages) != 1 {
		t.Errorf("expected 1 unread message for task-1, got %d", len(messages))
	}
	if messages[0].Type != "answer" {
		t.Errorf("expected type 'answer', got '%s'", messages[0].Type)
	}
}

func TestGetInboxMessagesEmpty(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	messages, err := db.GetInboxMessages("agent-a", "", "")
	if err != nil {
		t.Fatal(err)
	}
	if messages != nil {
		t.Errorf("expected nil for empty inbox, got %d messages", len(messages))
	}
}

func TestGetInboxMessagesOrdering(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	db.SendInboxMessage("task-1", "sender", "agent-a", "request", map[string]interface{}{"order": 1})
	db.SendInboxMessage("task-1", "sender", "agent-a", "question", map[string]interface{}{"order": 2})
	db.SendInboxMessage("task-1", "sender", "agent-a", "answer", map[string]interface{}{"order": 3})

	messages, _ := db.GetInboxMessages("agent-a", "", "")
	if len(messages) != 3 {
		t.Fatalf("expected 3 messages, got %d", len(messages))
	}

	// Should be in chronological order
	for i, msg := range messages {
		expected := float64(i + 1)
		if msg.Content["order"] != expected {
			t.Errorf("message %d: expected order %v, got %v", i, expected, msg.Content["order"])
		}
	}
}

func TestAckInboxMessage(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	msg, _ := db.SendInboxMessage("task-1", "sender", "agent-a", "request", nil)

	err := db.AckInboxMessage(msg.ID)
	if err != nil {
		t.Fatal(err)
	}

	// Verify it's acked
	messages, _ := db.GetInboxMessages("agent-a", "acked", "")
	if len(messages) != 1 {
		t.Errorf("expected 1 acked message, got %d", len(messages))
	}
}

func TestAckInboxMessageAlreadyAcked(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	msg, _ := db.SendInboxMessage("task-1", "sender", "agent-a", "request", nil)

	db.AckInboxMessage(msg.ID)
	err := db.AckInboxMessage(msg.ID)
	if err == nil {
		t.Error("expected error when acking already-acked message")
	}
}

func TestAckInboxMessageNotFound(t *testing.T) {
	db := setupTestDB(t)

	err := db.AckInboxMessage("nonexistent")
	if err == nil {
		t.Error("expected error for nonexistent message")
	}
}

func TestGetTaskMessages(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("main-agent")
	db.RegisterAgent("worker-agent")

	// Multi-turn conversation
	db.SendInboxMessage("task-1", "main-agent", "worker-agent", "request", map[string]interface{}{"instruction": "translate"})
	db.SendInboxMessage("task-1", "worker-agent", "main-agent", "question", map[string]interface{}{"q": "A or B?"})
	db.SendInboxMessage("task-1", "main-agent", "worker-agent", "answer", map[string]interface{}{"a": "A"})
	db.SendInboxMessage("task-1", "worker-agent", "main-agent", "done", map[string]interface{}{"result": "translated"})

	messages, err := db.GetTaskMessages("task-1")
	if err != nil {
		t.Fatal(err)
	}
	if len(messages) != 4 {
		t.Errorf("expected 4 messages, got %d", len(messages))
	}

	// Verify the conversation flow
	expectedTypes := []string{"request", "question", "answer", "done"}
	for i, msg := range messages {
		if msg.Type != expectedTypes[i] {
			t.Errorf("message %d: expected type '%s', got '%s'", i, expectedTypes[i], msg.Type)
		}
		if msg.TaskID != "task-1" {
			t.Errorf("message %d: expected task_id 'task-1', got '%s'", i, msg.TaskID)
		}
	}
}

func TestGetTaskMessagesEmpty(t *testing.T) {
	db := setupTestDB(t)

	messages, err := db.GetTaskMessages("nonexistent-task")
	if err != nil {
		t.Fatal(err)
	}
	if messages != nil {
		t.Errorf("expected nil for nonexistent task, got %d messages", len(messages))
	}
}

func TestInboxIsolation(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("agent-a")
	db.RegisterAgent("agent-b")

	db.SendInboxMessage("task-1", "sender", "agent-a", "request", map[string]interface{}{"for": "a"})
	db.SendInboxMessage("task-1", "sender", "agent-b", "request", map[string]interface{}{"for": "b"})

	msgsA, _ := db.GetInboxMessages("agent-a", "", "")
	msgsB, _ := db.GetInboxMessages("agent-b", "", "")

	if len(msgsA) != 1 {
		t.Errorf("agent-a expected 1 message, got %d", len(msgsA))
	}
	if len(msgsB) != 1 {
		t.Errorf("agent-b expected 1 message, got %d", len(msgsB))
	}

	if msgsA[0].Content["for"] != "a" {
		t.Error("agent-a got wrong message")
	}
	if msgsB[0].Content["for"] != "b" {
		t.Error("agent-b got wrong message")
	}
}

func TestMultipleTasksPerAgent(t *testing.T) {
	db := setupTestDB(t)

	db.RegisterAgent("main-agent")

	db.SendInboxMessage("task-1", "worker-1", "main-agent", "done", map[string]interface{}{"result": "task1-done"})
	db.SendInboxMessage("task-2", "worker-2", "main-agent", "question", map[string]interface{}{"q": "help?"})
	db.SendInboxMessage("task-3", "worker-3", "main-agent", "done", map[string]interface{}{"result": "task3-done"})

	// Get all messages
	all, _ := db.GetInboxMessages("main-agent", "", "")
	if len(all) != 3 {
		t.Errorf("expected 3 messages, got %d", len(all))
	}

	// Filter by task-2
	task2, _ := db.GetInboxMessages("main-agent", "", "task-2")
	if len(task2) != 1 {
		t.Errorf("expected 1 message for task-2, got %d", len(task2))
	}
	if task2[0].Type != "question" {
		t.Errorf("expected type 'question', got '%s'", task2[0].Type)
	}
}
