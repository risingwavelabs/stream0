# Stream0

The communication layer for AI agents. Every agent gets an inbox. Messages are point-to-point, grouped by task. Agents can have multi-turn conversations mid-task.

Not a message queue. Not a framework. Infrastructure for agents that need to talk to each other.

## Why

You have two agents on different machines. Agent A needs Agent B to do work. Halfway through, Agent B has a question. Today your options are:

- **Direct HTTP** — no persistence, no retry, no mid-task dialogue
- **Kafka/SQS** — designed for microservices, not agent conversations
- **Framework built-in** — only works inside one process

Stream0 solves this: send a message to an agent's inbox, it persists until read, agents can go back and forth, every message is tagged with a `task_id` so the main agent always knows which conversation a response belongs to.

## Quick Start

```bash
# Build and run
go build -o stream0 .
./stream0

# Server runs on http://127.0.0.1:8080
```

## 5-Minute Tutorial

### 1. Register two agents

```bash
curl -X POST http://localhost:8080/agents \
  -H "Content-Type: application/json" \
  -d '{"id": "main-agent"}'

curl -X POST http://localhost:8080/agents \
  -H "Content-Type: application/json" \
  -d '{"id": "translator"}'
```

### 2. Send a task

```bash
curl -X POST http://localhost:8080/agents/translator/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "translate-contract",
    "from": "main-agent",
    "type": "request",
    "content": {"text": "Translate this contract to Japanese"}
  }'
```

### 3. Translator reads inbox

```bash
curl "http://localhost:8080/agents/translator/inbox?status=unread"
```

### 4. Translator asks a question (mid-task dialogue)

```bash
curl -X POST http://localhost:8080/agents/main-agent/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "translate-contract",
    "from": "translator",
    "type": "question",
    "content": {"question": "Should indemnification be translated as A or B?"}
  }'
```

### 5. Main agent answers

```bash
curl -X POST http://localhost:8080/agents/translator/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "translate-contract",
    "from": "main-agent",
    "type": "answer",
    "content": {"answer": "Use B"}
  }'
```

### 6. Translator completes

```bash
curl -X POST http://localhost:8080/agents/main-agent/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "translate-contract",
    "from": "translator",
    "type": "done",
    "content": {"translated": "..."}
  }'
```

### 7. View full conversation

```bash
curl "http://localhost:8080/tasks/translate-contract/messages"
```

Returns all 4 messages in chronological order — the complete audit trail.

## API Reference

### Inbox Model (v2)

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/agents` | Register an agent (creates inbox) |
| `DELETE` | `/agents/{id}` | Delete an agent |
| `POST` | `/agents/{id}/inbox` | Send a message to an agent's inbox |
| `GET` | `/agents/{id}/inbox` | Read messages from an agent's inbox |
| `POST` | `/inbox/messages/{id}/ack` | Mark a message as read |
| `GET` | `/tasks/{task_id}/messages` | Get full conversation history |

#### Send a message

```
POST /agents/{agent_id}/inbox
{
  "task_id": "task-123",       // groups messages into a conversation
  "from": "sender-agent",      // who sent this
  "type": "request",           // request | question | answer | done | failed
  "content": { ... }           // any JSON
}
```

#### Read inbox

```
GET /agents/{agent_id}/inbox?status=unread&task_id=task-123&timeout=10
```

- `status` — filter by `unread` or `acked` (optional)
- `task_id` — filter by task (optional)
- `timeout` — long-poll in seconds, 0 for immediate (optional, max 30)

#### Message types

| Type | Meaning |
|------|---------|
| `request` | Start a task |
| `question` | Ask for clarification mid-task |
| `answer` | Respond to a question |
| `done` | Task completed successfully |
| `failed` | Task failed |

### Topic Model (v1)

The original topic-based pub/sub API is still available for broadcast and fan-out use cases. See [docs/v1-api.md](docs/v1-api.md) for details.

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/topics` | Create a topic |
| `GET` | `/topics` | List topics |
| `POST` | `/topics/{name}/messages` | Publish a message |
| `GET` | `/topics/{name}/messages` | Consume messages (long-polling) |
| `POST` | `/messages/{id}/ack` | Acknowledge a message |

## Python SDK

```bash
pip install -e sdk/python
```

```python
from stream0 import Agent

main = Agent("main-agent", url="http://localhost:8080")
translator = Agent("translator", url="http://localhost:8080")

main.register()
translator.register()

# Send task
main.send("translator", task_id="t1", msg_type="request",
          content={"text": "translate this"})

# Translator reads, asks question, gets answer
msgs = translator.receive(task_id="t1")
translator.ack(msgs[0]["id"])
translator.send("main-agent", task_id="t1", msg_type="question",
                content={"q": "A or B?"})

msgs = main.receive(task_id="t1")
main.send("translator", task_id="t1", msg_type="answer",
          content={"a": "use B"})

# Complete
translator.send("main-agent", task_id="t1", msg_type="done",
                content={"result": "translated"})

# Full history
history = main.history("t1")
```

## Configuration

### Config file (YAML)

```yaml
server:
  host: 0.0.0.0
  port: 8080

database:
  path: /var/lib/stream0/stream0.db

log:
  level: info
  format: json

auth:
  api_keys:
    - your-secret-key
```

### Environment variables

| Variable | Description | Default |
|----------|-------------|---------|
| `STREAM0_SERVER_HOST` | Bind address | `127.0.0.1` |
| `STREAM0_SERVER_PORT` | Port | `8080` |
| `STREAM0_DB_PATH` | Database path | `./stream0.db` |
| `STREAM0_LOG_LEVEL` | Log level | `info` |
| `STREAM0_LOG_FORMAT` | Log format (`json` or `text`) | `json` |
| `STREAM0_API_KEY` | API key for authentication | (none) |

### Authentication

When API keys are configured, all endpoints (except `/health`) require the `X-API-Key` header:

```bash
curl -H "X-API-Key: your-secret-key" http://localhost:8080/agents/my-agent/inbox
```

## Testing

```bash
# Go tests (87 tests)
go test -v ./...

# Python SDK unit tests (47 tests)
cd sdk/python && pip install -e ".[dev]" && pytest tests/test_client.py -v

# Python integration tests (25 tests, requires running server)
STREAM0_URL=http://localhost:8080 pytest tests/test_integration.py -v
```

## Architecture

```
Agent A                    Stream0                   Agent B
(main)                     (Go + SQLite)             (translator)
  │                            │                         │
  ├── POST /agents/B/inbox ──→ │ (persists message)      │
  │   type: request            │                         │
  │                            │ ←── GET /agents/B/inbox ┤
  │                            │     (returns message)   │
  │                            │                         │
  │   ┌── POST /agents/A/inbox ┤ ←────────────────────── ┤
  │   │   type: question       │                         │
  ├───┘                        │                         │
  │                            │                         │
  ├── POST /agents/B/inbox ──→ │ ──────────────────────→ │
  │   type: answer             │                         │
  │                            │                         │
  │   ┌── POST /agents/A/inbox ┤ ←────────────────────── ┤
  │   │   type: done           │                         │
  ├───┘                        │                         │
```

Every message carries the same `task_id`. The main agent always knows which conversation each response belongs to.

## Design Principles

- **Inbox model, not topic model.** Every agent has its own inbox. Messages are point-to-point.
- **HTTP-native.** No SDK required. curl works. Any language, any framework.
- **task_id is the conversation.** Like an email subject line — groups related messages.
- **Polling is fine.** Agents think for 30+ seconds. A few seconds of message latency is irrelevant.
- **Idempotency is the caller's responsibility.** Stream0 persists messages and delivers at-least-once. If your agent crashes and restarts, it will re-process unacked messages. Design accordingly.

## License

MIT
