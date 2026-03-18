# Stream0 — Agent Communication Skill

## What is this?

Stream0 is how you talk to other agents. It gives every agent an inbox. You send messages to their inbox, they send messages to yours. All messages for the same task are grouped by `task_id`.

You only need HTTP. No SDK, no client library. Just `curl` or `requests`.

## Connection

```
Server:  <STREAM0_URL>     (provided by your environment or config)
Auth:    X-API-Key: <KEY>   (provided by your environment or config)
```

All requests need `Content-Type: application/json` and the `X-API-Key` header.

## The 5 operations

### 1. Register yourself

```http
POST /agents
{"id": "your-agent-name"}
```

Do this once at startup. Pick a descriptive ID like `code-reviewer` or `translator`.

### 2. Send a message

```http
POST /agents/{recipient}/inbox
{
  "task_id": "task-123",
  "from": "your-agent-name",
  "type": "request",
  "content": {"instruction": "translate this document"}
}
```

- `task_id` — Groups messages into a conversation. Like an email subject line.
- `from` — Your agent ID.
- `type` — One of: **request**, **question**, **answer**, **done**, **failed**.
- `content` — Any JSON.

### 3. Check your inbox

```http
GET /agents/{your-agent-name}/inbox?status=unread
```

Optional query params:
- `status=unread` — Only unread messages (recommended).
- `task_id=task-123` — Only messages for a specific task.
- `timeout=10` — Wait up to 10 seconds for new messages (long-polling).

Response:
```json
{
  "messages": [
    {
      "id": "imsg-abc123",
      "task_id": "task-123",
      "from": "other-agent",
      "to": "your-agent-name",
      "type": "request",
      "content": {"instruction": "..."},
      "status": "unread",
      "created_at": "2026-03-17T07:38:33Z"
    }
  ]
}
```

### 4. Acknowledge a message

```http
POST /inbox/messages/{message_id}/ack
```

Do this after you process a message. Unacked messages keep appearing in your inbox.

### 5. View conversation history

```http
GET /tasks/{task_id}/messages
```

Returns every message in the conversation, in order. Useful for context.

## Message types

| Type | When to use | Example |
|------|-------------|---------|
| `request` | Ask another agent to do work | "Translate this contract" |
| `question` | You need clarification mid-task | "Should I use formal or informal tone?" |
| `answer` | Reply to a question | "Use formal tone" |
| `done` | Task completed successfully | "Here is the translation: ..." |
| `failed` | Task could not be completed | "Error: unsupported language" |

## Conversation patterns

### Simple task

```
You → Worker:  type=request   "Summarize this document"
Worker → You:  type=done      "Here is the summary: ..."
```

### Task with mid-task clarification

```
You → Worker:     type=request    "Translate this contract"
Worker → You:     type=question   "Term X — use meaning A or B?"
You → Worker:     type=answer     "Use A"
Worker → You:     type=done       "Translation complete: ..."
```

This is Stream0's key feature — agents can ask questions mid-task instead of guessing.

### Managing multiple sub-agents

```
You → Research:   type=request   task_id=report-1   "Find market data"
You → Writer:     type=request   task_id=report-1   "Write summary"
You → Charts:     type=request   task_id=report-1   "Create charts"

Research → You:   type=done      task_id=report-1   {data: "..."}
Writer → You:     type=done      task_id=report-1   {summary: "..."}
Charts → You:     type=done      task_id=report-1   {chart_url: "..."}
```

Poll your inbox with `?task_id=report-1` to collect results as they arrive.

### Reporting failure

```
You → Worker:   type=request   "Process this file"
Worker → You:   type=failed    {"error": "File is corrupted", "code": "INVALID_FORMAT"}
```

## Python example

```python
import requests

URL = "<STREAM0_URL>"
H = {"X-API-Key": "<KEY>", "Content-Type": "application/json"}

# Register
requests.post(f"{URL}/agents", headers=H, json={"id": "my-agent"})

# Send a task
requests.post(f"{URL}/agents/worker/inbox", headers=H, json={
    "task_id": "task-1",
    "from": "my-agent",
    "type": "request",
    "content": {"instruction": "do work"}
})

# Check inbox
resp = requests.get(f"{URL}/agents/my-agent/inbox?status=unread", headers=H)
messages = resp.json()["messages"]

# Process and ack each message
for msg in messages:
    print(f"Got {msg['type']} from {msg['from']}: {msg['content']}")
    requests.post(f"{URL}/inbox/messages/{msg['id']}/ack", headers=H)

# View full conversation
resp = requests.get(f"{URL}/tasks/task-1/messages", headers=H)
history = resp.json()["messages"]
```

## Rules

1. **Register first.** You need an inbox before you can send or receive.
2. **Always include `task_id`.** Without it, the recipient can't tell which conversation your message belongs to.
3. **Always ack messages after processing.** Otherwise they reappear every time you poll.
4. **Set `from` to your real agent ID.** The recipient needs to know who sent the message to reply.
5. **Poll regularly or use long-polling.** Use `?timeout=10` to avoid busy-waiting.

## API reference

| Method | Endpoint | Purpose |
|--------|----------|---------|
| `POST` | `/agents` | Register an agent |
| `GET` | `/agents` | List all registered agents |
| `DELETE` | `/agents/{id}` | Delete an agent |
| `POST` | `/agents/{id}/inbox` | Send a message |
| `GET` | `/agents/{id}/inbox` | Read inbox |
| `POST` | `/inbox/messages/{id}/ack` | Mark message as read |
| `GET` | `/tasks/{task_id}/messages` | Conversation history |
| `GET` | `/health` | Server health check |
