package main

import (
	"bytes"
	"encoding/json"
	"fmt"
	"net/http"
	"net/http/httptest"
	"path/filepath"
	"sync"
	"testing"
	"time"
)

func setupTestServer(t *testing.T) *Server {
	t.Helper()
	dbPath := filepath.Join(t.TempDir(), "test.db")
	db, err := NewDatabase(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	return NewServer(db, ServerConfig{Host: "127.0.0.1", Port: 0}, AuthConfig{})
}

func setupTestServerWithAuth(t *testing.T, keys []string) *Server {
	t.Helper()
	dbPath := filepath.Join(t.TempDir(), "test.db")
	db, err := NewDatabase(dbPath)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { db.Close() })
	return NewServer(db, ServerConfig{Host: "127.0.0.1", Port: 0}, AuthConfig{APIKeys: keys})
}

func doRequest(s *Server, method, path string, body interface{}) *httptest.ResponseRecorder {
	var reqBody *bytes.Reader
	if body != nil {
		data, _ := json.Marshal(body)
		reqBody = bytes.NewReader(data)
	} else {
		reqBody = bytes.NewReader(nil)
	}

	req := httptest.NewRequest(method, path, reqBody)
	req.Header.Set("Content-Type", "application/json")
	w := httptest.NewRecorder()
	s.router.ServeHTTP(w, req)
	return w
}

func doRequestWithAuth(s *Server, method, path string, body interface{}, apiKey string) *httptest.ResponseRecorder {
	var reqBody *bytes.Reader
	if body != nil {
		data, _ := json.Marshal(body)
		reqBody = bytes.NewReader(data)
	} else {
		reqBody = bytes.NewReader(nil)
	}

	req := httptest.NewRequest(method, path, reqBody)
	req.Header.Set("Content-Type", "application/json")
	if apiKey != "" {
		req.Header.Set("X-API-Key", apiKey)
	}
	w := httptest.NewRecorder()
	s.router.ServeHTTP(w, req)
	return w
}

func parseResponse(t *testing.T, w *httptest.ResponseRecorder) map[string]interface{} {
	t.Helper()
	var result map[string]interface{}
	if err := json.Unmarshal(w.Body.Bytes(), &result); err != nil {
		t.Fatalf("failed to parse response: %v, body: %s", err, w.Body.String())
	}
	return result
}

// --- Health ---

func TestHealthEndpoint(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "GET", "/health", nil)

	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}

	result := parseResponse(t, w)
	if result["status"] != "healthy" {
		t.Error("expected status healthy")
	}
}

// --- Topics ---

func TestCreateTopicEndpoint(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "POST", "/topics", map[string]interface{}{
		"name":           "test-topic",
		"retention_days": 7,
	})

	if w.Code != http.StatusCreated {
		t.Errorf("expected 201, got %d: %s", w.Code, w.Body.String())
	}

	result := parseResponse(t, w)
	if result["name"] != "test-topic" {
		t.Error("expected topic name 'test-topic'")
	}
}

func TestCreateTopicMissingName(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "POST", "/topics", map[string]interface{}{})

	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d", w.Code)
	}
}

func TestListTopicsEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "topic-a"})
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "topic-b"})

	w := doRequest(s, "GET", "/topics", nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}

	var topics []map[string]interface{}
	json.Unmarshal(w.Body.Bytes(), &topics)
	if len(topics) != 2 {
		t.Errorf("expected 2 topics, got %d", len(topics))
	}
}

func TestGetTopicEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "my-topic"})

	w := doRequest(s, "GET", "/topics/my-topic", nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}

	result := parseResponse(t, w)
	if result["name"] != "my-topic" {
		t.Error("expected topic name 'my-topic'")
	}
}

func TestGetTopicNotFoundEndpoint(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "GET", "/topics/nonexistent", nil)
	if w.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", w.Code)
	}
}

// --- Produce ---

func TestProduceMessageEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "events"})

	w := doRequest(s, "POST", "/topics/events/messages", map[string]interface{}{
		"payload": map[string]interface{}{"action": "click"},
		"headers": map[string]string{"trace-id": "xyz"},
	})

	if w.Code != http.StatusCreated {
		t.Errorf("expected 201, got %d: %s", w.Code, w.Body.String())
	}

	result := parseResponse(t, w)
	if result["message_id"] == nil || result["message_id"] == "" {
		t.Error("expected message_id in response")
	}
	if result["offset"].(float64) != 1 {
		t.Error("expected offset 1")
	}
}

func TestProduceToNonexistentTopic(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "POST", "/topics/ghost/messages", map[string]interface{}{
		"payload": map[string]interface{}{"x": 1},
	})

	if w.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", w.Code)
	}
}

func TestProduceMissingPayload(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "events"})

	w := doRequest(s, "POST", "/topics/events/messages", map[string]interface{}{})
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d", w.Code)
	}
}

// --- Consume ---

func TestConsumeMessagesEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "events"})
	doRequest(s, "POST", "/topics/events/messages", map[string]interface{}{
		"payload": map[string]interface{}{"n": 1},
	})
	doRequest(s, "POST", "/topics/events/messages", map[string]interface{}{
		"payload": map[string]interface{}{"n": 2},
	})

	w := doRequest(s, "GET", "/topics/events/messages?group=g1&timeout=1", nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}

	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 2 {
		t.Errorf("expected 2 messages, got %d", len(messages))
	}
}

func TestConsumeEmptyTopic(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "empty"})

	w := doRequest(s, "GET", "/topics/empty/messages?group=g1&timeout=0.1", nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}

	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 0 {
		t.Errorf("expected 0 messages, got %d", len(messages))
	}
}

func TestConsumeMissingGroup(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "events"})

	w := doRequest(s, "GET", "/topics/events/messages", nil)
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d", w.Code)
	}
}

// --- Acknowledge ---

func TestAcknowledgeEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "events"})
	doRequest(s, "POST", "/topics/events/messages", map[string]interface{}{
		"payload": map[string]interface{}{"n": 1},
	})

	// Consume
	w := doRequest(s, "GET", "/topics/events/messages?group=g1&timeout=1", nil)
	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	msg := messages[0].(map[string]interface{})
	msgID := msg["id"].(string)

	// Acknowledge
	w = doRequest(s, "POST", fmt.Sprintf("/messages/%s/ack", msgID), map[string]interface{}{
		"group": "g1",
	})
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
}

// --- Auth ---

func TestAPIKeyAuthRequired(t *testing.T) {
	s := setupTestServerWithAuth(t, []string{"secret-key-123"})

	// No key - should be rejected
	w := doRequest(s, "GET", "/topics", nil)
	if w.Code != http.StatusUnauthorized {
		t.Errorf("expected 401 without key, got %d", w.Code)
	}

	// Wrong key
	w = doRequestWithAuth(s, "GET", "/topics", nil, "wrong-key")
	if w.Code != http.StatusUnauthorized {
		t.Errorf("expected 401 with wrong key, got %d", w.Code)
	}

	// Correct key
	w = doRequestWithAuth(s, "GET", "/topics", nil, "secret-key-123")
	if w.Code != http.StatusOK {
		t.Errorf("expected 200 with correct key, got %d", w.Code)
	}
}

func TestHealthNoAuthRequired(t *testing.T) {
	s := setupTestServerWithAuth(t, []string{"secret-key-123"})

	w := doRequest(s, "GET", "/health", nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200 for health without auth, got %d", w.Code)
	}
}

// --- Request-Reply ---

func TestRequestReplyEndToEnd(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "tasks"})

	var requestResult map[string]interface{}
	var wg sync.WaitGroup
	wg.Add(1)

	// Requester: send request (blocks waiting for reply)
	go func() {
		defer wg.Done()
		w := doRequest(s, "POST", "/topics/tasks/request", map[string]interface{}{
			"payload": map[string]interface{}{"question": "what is 2+2?"},
			"timeout": 10,
		})
		if w.Code != http.StatusOK {
			t.Errorf("request expected 200, got %d: %s", w.Code, w.Body.String())
			return
		}
		json.Unmarshal(w.Body.Bytes(), &requestResult)
	}()

	// Give the request time to be published
	time.Sleep(300 * time.Millisecond)

	// Responder: consume the message
	w := doRequest(s, "GET", "/topics/tasks/messages?group=workers&timeout=5", nil)
	if w.Code != http.StatusOK {
		t.Fatalf("consume expected 200, got %d: %s", w.Code, w.Body.String())
	}

	consumeResult := parseResponse(t, w)
	messages := consumeResult["messages"].([]interface{})
	if len(messages) != 1 {
		t.Fatalf("expected 1 message, got %d", len(messages))
	}

	msg := messages[0].(map[string]interface{})
	msgID := msg["id"].(string)

	// Verify correlation_id is in headers
	msgHeaders := msg["headers"].(map[string]interface{})
	if msgHeaders["correlation_id"] == nil || msgHeaders["correlation_id"] == "" {
		t.Fatal("expected correlation_id in message headers")
	}

	// Responder: send reply
	w = doRequest(s, "POST", fmt.Sprintf("/messages/%s/reply", msgID), map[string]interface{}{
		"payload": map[string]interface{}{"answer": "4"},
		"group":   "workers",
	})
	if w.Code != http.StatusOK {
		t.Fatalf("reply expected 200, got %d: %s", w.Code, w.Body.String())
	}

	// Wait for requester to finish
	wg.Wait()

	// Verify the request got the reply
	if requestResult == nil {
		t.Fatal("request result was nil")
	}
	if requestResult["correlation_id"] == nil {
		t.Error("expected correlation_id in request result")
	}

	reply := requestResult["reply"].(map[string]interface{})
	replyPayload := reply["payload"].(map[string]interface{})
	if replyPayload["answer"] != "4" {
		t.Errorf("expected answer '4', got %v", replyPayload["answer"])
	}
}

func TestRequestReplyTimeout(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "tasks"})

	// Send request with very short timeout, no one will reply
	w := doRequest(s, "POST", "/topics/tasks/request", map[string]interface{}{
		"payload": map[string]interface{}{"question": "hello?"},
		"timeout": 0.5,
	})

	if w.Code != http.StatusGatewayTimeout {
		t.Errorf("expected 504 (gateway timeout), got %d: %s", w.Code, w.Body.String())
	}

	result := parseResponse(t, w)
	if result["correlation_id"] == nil {
		t.Error("expected correlation_id in timeout response")
	}
	if result["request_id"] == nil {
		t.Error("expected request_id in timeout response")
	}
}

func TestRequestReplyTopicNotFound(t *testing.T) {
	s := setupTestServer(t)

	w := doRequest(s, "POST", "/topics/nonexistent/request", map[string]interface{}{
		"payload": map[string]interface{}{"q": "test"},
	})

	if w.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", w.Code)
	}
}

func TestRequestReplyMissingPayload(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "tasks"})

	w := doRequest(s, "POST", "/topics/tasks/request", map[string]interface{}{})
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d", w.Code)
	}
}

func TestReplyToNonexistentMessage(t *testing.T) {
	s := setupTestServer(t)

	w := doRequest(s, "POST", "/messages/fake-id/reply", map[string]interface{}{
		"payload": map[string]interface{}{"x": 1},
	})

	if w.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", w.Code)
	}
}

func TestReplyToMessageWithoutCorrelationID(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "events"})

	// Publish a regular message (no correlation_id)
	w := doRequest(s, "POST", "/topics/events/messages", map[string]interface{}{
		"payload": map[string]interface{}{"n": 1},
	})
	result := parseResponse(t, w)
	msgID := result["message_id"].(string)

	// Try to reply to it
	w = doRequest(s, "POST", fmt.Sprintf("/messages/%s/reply", msgID), map[string]interface{}{
		"payload": map[string]interface{}{"x": 1},
	})

	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d: %s", w.Code, w.Body.String())
	}
}

func TestReplyWithAck(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "tasks"})

	// Publish a request message manually with correlation_id
	w := doRequest(s, "POST", "/topics/tasks/messages", map[string]interface{}{
		"payload": map[string]interface{}{"q": "test"},
		"headers": map[string]string{"correlation_id": "corr-test"},
	})
	produceResult := parseResponse(t, w)
	msgID := produceResult["message_id"].(string)

	// Consume it
	w = doRequest(s, "GET", "/topics/tasks/messages?group=workers&timeout=1", nil)
	consumeResult := parseResponse(t, w)
	messages := consumeResult["messages"].([]interface{})
	if len(messages) != 1 {
		t.Fatalf("expected 1 message, got %d", len(messages))
	}

	// Reply with ack
	w = doRequest(s, "POST", fmt.Sprintf("/messages/%s/reply", msgID), map[string]interface{}{
		"payload": map[string]interface{}{"a": "result"},
		"group":   "workers",
	})
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d: %s", w.Code, w.Body.String())
	}

	// Verify the message is acknowledged (no more messages to consume)
	w = doRequest(s, "GET", "/topics/tasks/messages?group=workers&timeout=0.1", nil)
	consumeResult = parseResponse(t, w)
	messages = consumeResult["messages"].([]interface{})
	if len(messages) != 0 {
		t.Errorf("expected 0 messages after reply+ack, got %d", len(messages))
	}
}

func TestMultipleConcurrentRequests(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "tasks"})

	numRequests := 5
	results := make([]map[string]interface{}, numRequests)
	var wg sync.WaitGroup

	// Send multiple requests concurrently
	for i := 0; i < numRequests; i++ {
		wg.Add(1)
		go func(idx int) {
			defer wg.Done()
			w := doRequest(s, "POST", "/topics/tasks/request", map[string]interface{}{
				"payload": map[string]interface{}{"question": fmt.Sprintf("q%d", idx)},
				"timeout": 10,
			})
			if w.Code == http.StatusOK {
				json.Unmarshal(w.Body.Bytes(), &results[idx])
			}
		}(i)
	}

	// Give requests time to publish
	time.Sleep(500 * time.Millisecond)

	// Consume and reply to all messages
	for i := 0; i < numRequests; i++ {
		w := doRequest(s, "GET", "/topics/tasks/messages?group=workers&timeout=5&max=1", nil)
		if w.Code != http.StatusOK {
			continue
		}

		consumeResult := parseResponse(t, w)
		messages := consumeResult["messages"].([]interface{})
		if len(messages) == 0 {
			continue
		}

		msg := messages[0].(map[string]interface{})
		msgID := msg["id"].(string)

		doRequest(s, "POST", fmt.Sprintf("/messages/%s/reply", msgID), map[string]interface{}{
			"payload": map[string]interface{}{"answer": fmt.Sprintf("a%d", i)},
			"group":   "workers",
		})
	}

	wg.Wait()

	// Count successful replies
	successCount := 0
	for _, r := range results {
		if r != nil && r["reply"] != nil {
			successCount++
		}
	}

	if successCount != numRequests {
		t.Errorf("expected %d successful replies, got %d", numRequests, successCount)
	}
}

// --- v2 Inbox Model Endpoint Tests ---

func TestRegisterAgentEndpoint(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "POST", "/agents", map[string]interface{}{"id": "agent-1"})

	if w.Code != http.StatusCreated {
		t.Errorf("expected 201, got %d: %s", w.Code, w.Body.String())
	}

	result := parseResponse(t, w)
	if result["id"] != "agent-1" {
		t.Error("expected agent id 'agent-1'")
	}
}

func TestRegisterAgentMissingID(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "POST", "/agents", map[string]interface{}{})

	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d", w.Code)
	}
}

func TestRegisterAgentDuplicateEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "agent-1"})
	w := doRequest(s, "POST", "/agents", map[string]interface{}{"id": "agent-1"})

	// Should succeed (idempotent)
	if w.Code != http.StatusCreated {
		t.Errorf("expected 201 for duplicate registration, got %d", w.Code)
	}
}

func TestDeleteAgentEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "agent-1"})

	w := doRequest(s, "DELETE", "/agents/agent-1", nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d: %s", w.Code, w.Body.String())
	}
}

func TestDeleteAgentNotFoundEndpoint(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "DELETE", "/agents/nonexistent", nil)

	if w.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", w.Code)
	}
}

func TestSendInboxMessageEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	w := doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1",
		"from":    "main-agent",
		"type":    "request",
		"content": map[string]interface{}{"instruction": "do work"},
	})

	if w.Code != http.StatusCreated {
		t.Errorf("expected 201, got %d: %s", w.Code, w.Body.String())
	}

	result := parseResponse(t, w)
	if result["message_id"] == nil || result["message_id"] == "" {
		t.Error("expected message_id in response")
	}
}

func TestSendInboxMessageToNonexistentAgent(t *testing.T) {
	s := setupTestServer(t)

	w := doRequest(s, "POST", "/agents/ghost/inbox", map[string]interface{}{
		"task_id": "task-1",
		"from":    "main",
		"type":    "request",
	})

	if w.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", w.Code)
	}
}

func TestSendInboxMessageInvalidType(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	w := doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1",
		"from":    "main",
		"type":    "invalid_type",
	})

	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400, got %d: %s", w.Code, w.Body.String())
	}
}

func TestSendInboxMessageMissingFields(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	// Missing task_id
	w := doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"from": "main",
		"type": "request",
	})
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400 for missing task_id, got %d", w.Code)
	}

	// Missing from
	w = doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1",
		"type":    "request",
	})
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400 for missing from, got %d", w.Code)
	}

	// Missing type
	w = doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1",
		"from":    "main",
	})
	if w.Code != http.StatusBadRequest {
		t.Errorf("expected 400 for missing type, got %d", w.Code)
	}
}

func TestGetInboxMessagesEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "main", "type": "request",
		"content": map[string]interface{}{"n": 1},
	})
	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-2", "from": "main", "type": "request",
		"content": map[string]interface{}{"n": 2},
	})

	w := doRequest(s, "GET", "/agents/worker/inbox", nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d: %s", w.Code, w.Body.String())
	}

	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 2 {
		t.Errorf("expected 2 messages, got %d", len(messages))
	}
}

func TestGetInboxMessagesFilterStatus(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "main", "type": "request",
	})
	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-2", "from": "main", "type": "request",
	})

	// Get first message ID and ack it
	w := doRequest(s, "GET", "/agents/worker/inbox?status=unread", nil)
	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	firstMsgID := messages[0].(map[string]interface{})["id"].(string)

	doRequest(s, "POST", fmt.Sprintf("/inbox/messages/%s/ack", firstMsgID), nil)

	// Only unread
	w = doRequest(s, "GET", "/agents/worker/inbox?status=unread", nil)
	result = parseResponse(t, w)
	messages = result["messages"].([]interface{})
	if len(messages) != 1 {
		t.Errorf("expected 1 unread message, got %d", len(messages))
	}
}

func TestGetInboxMessagesFilterTaskID(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "main", "type": "request",
	})
	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-2", "from": "main", "type": "request",
	})
	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "main", "type": "answer",
	})

	w := doRequest(s, "GET", "/agents/worker/inbox?task_id=task-1", nil)
	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 2 {
		t.Errorf("expected 2 messages for task-1, got %d", len(messages))
	}
}

func TestGetInboxMessagesNonexistentAgent(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "GET", "/agents/ghost/inbox", nil)

	if w.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", w.Code)
	}
}

func TestGetInboxMessagesLongPolling(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	var wg sync.WaitGroup
	var pollResult map[string]interface{}

	// Start long-polling
	wg.Add(1)
	go func() {
		defer wg.Done()
		w := doRequest(s, "GET", "/agents/worker/inbox?status=unread&timeout=10", nil)
		json.Unmarshal(w.Body.Bytes(), &pollResult)
	}()

	// Send a message after a short delay
	time.Sleep(300 * time.Millisecond)
	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "main", "type": "request",
		"content": map[string]interface{}{"data": "hello"},
	})

	wg.Wait()

	if pollResult == nil {
		t.Fatal("poll result was nil")
	}
	messages := pollResult["messages"].([]interface{})
	if len(messages) != 1 {
		t.Errorf("expected 1 message from long-poll, got %d", len(messages))
	}
}

func TestGetInboxMessagesLongPollingTimeout(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	start := time.Now()
	w := doRequest(s, "GET", "/agents/worker/inbox?status=unread&timeout=1", nil)
	elapsed := time.Since(start)

	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}

	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 0 {
		t.Errorf("expected 0 messages on timeout, got %d", len(messages))
	}

	if elapsed < 800*time.Millisecond {
		t.Errorf("long-poll returned too quickly: %v", elapsed)
	}
}

func TestAckInboxMessageEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "main", "type": "request",
	})

	// Get the message ID
	w := doRequest(s, "GET", "/agents/worker/inbox?status=unread", nil)
	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	msgID := messages[0].(map[string]interface{})["id"].(string)

	// Ack
	w = doRequest(s, "POST", fmt.Sprintf("/inbox/messages/%s/ack", msgID), nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d: %s", w.Code, w.Body.String())
	}

	// Verify it's acked
	w = doRequest(s, "GET", "/agents/worker/inbox?status=unread", nil)
	result = parseResponse(t, w)
	messages = result["messages"].([]interface{})
	if len(messages) != 0 {
		t.Errorf("expected 0 unread after ack, got %d", len(messages))
	}
}

func TestAckInboxMessageNotFoundEndpoint(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "POST", "/inbox/messages/nonexistent/ack", nil)

	if w.Code != http.StatusNotFound {
		t.Errorf("expected 404, got %d", w.Code)
	}
}

func TestGetTaskMessagesEndpoint(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "main-agent"})
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "translator"})

	// Full translation conversation
	doRequest(s, "POST", "/agents/translator/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "main-agent", "type": "request",
		"content": map[string]interface{}{"text": "translate this contract"},
	})
	doRequest(s, "POST", "/agents/main-agent/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "translator", "type": "question",
		"content": map[string]interface{}{"q": "A or B?"},
	})
	doRequest(s, "POST", "/agents/translator/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "main-agent", "type": "answer",
		"content": map[string]interface{}{"a": "use A"},
	})
	doRequest(s, "POST", "/agents/main-agent/inbox", map[string]interface{}{
		"task_id": "task-1", "from": "translator", "type": "done",
		"content": map[string]interface{}{"result": "translated document"},
	})

	// Get full conversation
	w := doRequest(s, "GET", "/tasks/task-1/messages", nil)
	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d: %s", w.Code, w.Body.String())
	}

	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 4 {
		t.Errorf("expected 4 messages, got %d", len(messages))
	}

	// Verify conversation flow
	expectedTypes := []string{"request", "question", "answer", "done"}
	for i, m := range messages {
		msg := m.(map[string]interface{})
		if msg["type"] != expectedTypes[i] {
			t.Errorf("message %d: expected type '%s', got '%s'", i, expectedTypes[i], msg["type"])
		}
	}
}

func TestGetTaskMessagesEmptyEndpoint(t *testing.T) {
	s := setupTestServer(t)
	w := doRequest(s, "GET", "/tasks/nonexistent/messages", nil)

	if w.Code != http.StatusOK {
		t.Errorf("expected 200, got %d", w.Code)
	}

	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 0 {
		t.Errorf("expected 0 messages, got %d", len(messages))
	}
}

func TestMultiTurnConversationEndToEnd(t *testing.T) {
	s := setupTestServer(t)

	// Register agents
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "orchestrator"})
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "code-reviewer"})

	// Orchestrator sends a code review task
	doRequest(s, "POST", "/agents/code-reviewer/inbox", map[string]interface{}{
		"task_id": "review-pr-42",
		"from":    "orchestrator",
		"type":    "request",
		"content": map[string]interface{}{
			"pr_url":  "github.com/example/repo/pull/42",
			"files":   []string{"main.go", "server.go"},
		},
	})

	// Code reviewer picks up the message
	w := doRequest(s, "GET", "/agents/code-reviewer/inbox?status=unread", nil)
	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 1 {
		t.Fatalf("expected 1 message, got %d", len(messages))
	}

	msg := messages[0].(map[string]interface{})
	if msg["task_id"] != "review-pr-42" {
		t.Error("expected task_id 'review-pr-42'")
	}

	// Ack the received message
	doRequest(s, "POST", fmt.Sprintf("/inbox/messages/%s/ack", msg["id"].(string)), nil)

	// Code reviewer asks a question
	doRequest(s, "POST", "/agents/orchestrator/inbox", map[string]interface{}{
		"task_id": "review-pr-42",
		"from":    "code-reviewer",
		"type":    "question",
		"content": map[string]interface{}{
			"question": "The function on line 42 shadows a variable. Intentional?",
		},
	})

	// Orchestrator polls for messages from this task
	w = doRequest(s, "GET", "/agents/orchestrator/inbox?status=unread&task_id=review-pr-42", nil)
	result = parseResponse(t, w)
	messages = result["messages"].([]interface{})
	if len(messages) != 1 {
		t.Fatalf("expected 1 message, got %d", len(messages))
	}

	questionMsg := messages[0].(map[string]interface{})
	doRequest(s, "POST", fmt.Sprintf("/inbox/messages/%s/ack", questionMsg["id"].(string)), nil)

	// Orchestrator answers
	doRequest(s, "POST", "/agents/code-reviewer/inbox", map[string]interface{}{
		"task_id": "review-pr-42",
		"from":    "orchestrator",
		"type":    "answer",
		"content": map[string]interface{}{
			"answer": "Yes, intentional. It's a test override.",
		},
	})

	// Code reviewer gets the answer
	w = doRequest(s, "GET", "/agents/code-reviewer/inbox?status=unread", nil)
	result = parseResponse(t, w)
	messages = result["messages"].([]interface{})
	if len(messages) != 1 {
		t.Fatalf("expected 1 message, got %d", len(messages))
	}

	answerMsg := messages[0].(map[string]interface{})
	doRequest(s, "POST", fmt.Sprintf("/inbox/messages/%s/ack", answerMsg["id"].(string)), nil)

	// Code reviewer completes the review
	doRequest(s, "POST", "/agents/orchestrator/inbox", map[string]interface{}{
		"task_id": "review-pr-42",
		"from":    "code-reviewer",
		"type":    "done",
		"content": map[string]interface{}{
			"approved": true,
			"comments": []string{"LGTM, variable shadow is intentional"},
		},
	})

	// Verify full conversation history
	w = doRequest(s, "GET", "/tasks/review-pr-42/messages", nil)
	result = parseResponse(t, w)
	messages = result["messages"].([]interface{})
	if len(messages) != 4 {
		t.Errorf("expected 4 messages in conversation, got %d", len(messages))
	}

	expectedFlow := []struct {
		from, to, msgType string
	}{
		{"orchestrator", "code-reviewer", "request"},
		{"code-reviewer", "orchestrator", "question"},
		{"orchestrator", "code-reviewer", "answer"},
		{"code-reviewer", "orchestrator", "done"},
	}

	for i, expected := range expectedFlow {
		msg := messages[i].(map[string]interface{})
		if msg["from"] != expected.from {
			t.Errorf("msg %d: expected from '%s', got '%s'", i, expected.from, msg["from"])
		}
		if msg["to"] != expected.to {
			t.Errorf("msg %d: expected to '%s', got '%s'", i, expected.to, msg["to"])
		}
		if msg["type"] != expected.msgType {
			t.Errorf("msg %d: expected type '%s', got '%s'", i, expected.msgType, msg["type"])
		}
	}
}

func TestMultipleSubAgentsConcurrent(t *testing.T) {
	s := setupTestServer(t)

	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "main"})
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "research"})
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "writer"})
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "charts"})

	// Main agent sends tasks to 3 sub-agents, all tagged with the same task_id
	doRequest(s, "POST", "/agents/research/inbox", map[string]interface{}{
		"task_id": "report-1", "from": "main", "type": "request",
		"content": map[string]interface{}{"instruction": "find data"},
	})
	doRequest(s, "POST", "/agents/writer/inbox", map[string]interface{}{
		"task_id": "report-1", "from": "main", "type": "request",
		"content": map[string]interface{}{"instruction": "write summary"},
	})
	doRequest(s, "POST", "/agents/charts/inbox", map[string]interface{}{
		"task_id": "report-1", "from": "main", "type": "request",
		"content": map[string]interface{}{"instruction": "create charts"},
	})

	// Each sub-agent completes
	doRequest(s, "POST", "/agents/main/inbox", map[string]interface{}{
		"task_id": "report-1", "from": "research", "type": "done",
		"content": map[string]interface{}{"data": "market is $5B"},
	})
	doRequest(s, "POST", "/agents/main/inbox", map[string]interface{}{
		"task_id": "report-1", "from": "writer", "type": "done",
		"content": map[string]interface{}{"summary": "Report written"},
	})
	doRequest(s, "POST", "/agents/main/inbox", map[string]interface{}{
		"task_id": "report-1", "from": "charts", "type": "done",
		"content": map[string]interface{}{"chart_url": "chart.png"},
	})

	// Main agent sees all 3 completions
	w := doRequest(s, "GET", "/agents/main/inbox?task_id=report-1", nil)
	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 3 {
		t.Errorf("expected 3 done messages, got %d", len(messages))
	}

	// Full task history shows all 6 messages
	w = doRequest(s, "GET", "/tasks/report-1/messages", nil)
	result = parseResponse(t, w)
	messages = result["messages"].([]interface{})
	if len(messages) != 6 {
		t.Errorf("expected 6 total messages, got %d", len(messages))
	}
}

func TestV2AuthProtection(t *testing.T) {
	s := setupTestServerWithAuth(t, []string{"secret-key"})

	// Without key — should fail
	w := doRequest(s, "POST", "/agents", map[string]interface{}{"id": "agent-1"})
	if w.Code != http.StatusUnauthorized {
		t.Errorf("expected 401 without key, got %d", w.Code)
	}

	// With key — should succeed
	w = doRequestWithAuth(s, "POST", "/agents", map[string]interface{}{"id": "agent-1"}, "secret-key")
	if w.Code != http.StatusCreated {
		t.Errorf("expected 201 with key, got %d", w.Code)
	}

	// Inbox endpoints also require key
	w = doRequest(s, "POST", "/agents/agent-1/inbox", map[string]interface{}{
		"task_id": "t1", "from": "other", "type": "request",
	})
	if w.Code != http.StatusUnauthorized {
		t.Errorf("expected 401 for inbox without key, got %d", w.Code)
	}
}

func TestAllMessageTypes(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/agents", map[string]interface{}{"id": "worker"})

	types := []string{"request", "question", "answer", "done", "failed"}
	for _, typ := range types {
		w := doRequest(s, "POST", "/agents/worker/inbox", map[string]interface{}{
			"task_id": "task-1", "from": "main", "type": typ,
		})
		if w.Code != http.StatusCreated {
			t.Errorf("type '%s': expected 201, got %d: %s", typ, w.Code, w.Body.String())
		}
	}

	// Verify all 5 messages arrived
	w := doRequest(s, "GET", "/agents/worker/inbox", nil)
	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 5 {
		t.Errorf("expected 5 messages (one per type), got %d", len(messages))
	}
}

// --- Full produce/consume/ack flow ---

func TestFullProduceConsumeAckFlow(t *testing.T) {
	s := setupTestServer(t)
	doRequest(s, "POST", "/topics", map[string]interface{}{"name": "work"})

	// Produce 5 messages
	for i := 1; i <= 5; i++ {
		w := doRequest(s, "POST", "/topics/work/messages", map[string]interface{}{
			"payload": map[string]interface{}{"task": i},
		})
		if w.Code != http.StatusCreated {
			t.Fatalf("produce failed: %d", w.Code)
		}
	}

	// Consume all
	w := doRequest(s, "GET", "/topics/work/messages?group=workers&timeout=1&max=5", nil)
	result := parseResponse(t, w)
	messages := result["messages"].([]interface{})
	if len(messages) != 5 {
		t.Fatalf("expected 5 messages, got %d", len(messages))
	}

	// Ack all
	for _, m := range messages {
		msg := m.(map[string]interface{})
		doRequest(s, "POST", fmt.Sprintf("/messages/%s/ack", msg["id"].(string)), map[string]interface{}{
			"group": "workers",
		})
	}

	// Verify no more messages
	w = doRequest(s, "GET", "/topics/work/messages?group=workers&timeout=0.1", nil)
	result = parseResponse(t, w)
	messages = result["messages"].([]interface{})
	if len(messages) != 0 {
		t.Errorf("expected 0 messages after ack, got %d", len(messages))
	}
}
