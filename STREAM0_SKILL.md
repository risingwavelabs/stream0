# Stream0 — Agent Communication Skill

You are an AI agent that needs to communicate with other agents through Stream0. Stream0 is an HTTP inbox system. Every agent has an inbox. You send messages to other agents' inboxes. All messages are grouped by `task_id`.

## Setup

**Server**: `http://100.48.110.189:8080`
**Auth**: Include `X-API-Key: YOUR_KEY` header on every request (except `/health`).

## Step 1: Register yourself

Before sending or receiving messages, register your agent:

```bash
curl -X POST http://SERVER/agents \
  -H "X-API-Key: KEY" -H "Content-Type: application/json" \
  -d '{"id": "your-agent-name"}'
```

Pick a unique, descriptive ID like `code-reviewer`, `translator`, `research-agent`.

## Step 2: Send a message

To send a message to another agent's inbox:

```bash
curl -X POST http://SERVER/agents/TARGET_AGENT/inbox \
  -H "X-API-Key: KEY" -H "Content-Type: application/json" \
  -d '{
    "task_id": "unique-task-id",
    "from": "your-agent-name",
    "type": "request",
    "content": {"instruction": "what you want done"}
  }'
```

**Fields:**
- `task_id` — A unique ID for this conversation. Use it to group all messages about the same task.
- `from` — Your agent ID.
- `type` — One of: `request`, `question`, `answer`, `done`, `failed`.
- `content` — Any JSON object.

## Step 3: Read your inbox

```bash
# Get all unread messages
curl "http://SERVER/agents/your-agent-name/inbox?status=unread" \
  -H "X-API-Key: KEY"

# Get unread messages for a specific task
curl "http://SERVER/agents/your-agent-name/inbox?status=unread&task_id=TASK_ID" \
  -H "X-API-Key: KEY"

# Long-poll (wait up to 10 seconds for new messages)
curl "http://SERVER/agents/your-agent-name/inbox?status=unread&timeout=10" \
  -H "X-API-Key: KEY"
```

Response:
```json
{
  "messages": [
    {
      "id": "imsg-abc123",
      "task_id": "task-1",
      "from": "other-agent",
      "to": "your-agent-name",
      "type": "request",
      "content": {"instruction": "..."},
      "status": "unread"
    }
  ]
}
```

## Step 4: Acknowledge messages

After processing a message, mark it as read so it doesn't appear again:

```bash
curl -X POST http://SERVER/inbox/messages/MESSAGE_ID/ack \
  -H "X-API-Key: KEY"
```

## Step 5: View conversation history

See all messages for a task in chronological order:

```bash
curl "http://SERVER/tasks/TASK_ID/messages" -H "X-API-Key: KEY"
```

## Message types and when to use them

| Type | When to use |
|------|-------------|
| `request` | You are asking another agent to do work |
| `question` | You are working on a task but need clarification |
| `answer` | You are responding to a question |
| `done` | You finished the task successfully |
| `failed` | You could not complete the task |

## Common patterns

### Pattern 1: Simple task (request → done)

```
Agent A → Agent B: type=request  "Summarize this document"
Agent B → Agent A: type=done     "Here is the summary: ..."
```

### Pattern 2: Task with clarification (request → question → answer → done)

```
Agent A → Agent B: type=request   "Translate this contract"
Agent B → Agent A: type=question  "Should I use formal or informal tone?"
Agent A → Agent B: type=answer    "Formal"
Agent B → Agent A: type=done      "Here is the translation: ..."
```

### Pattern 3: Multiple sub-agents

```
Main → Research:  type=request  task_id=report-1  "Find market data"
Main → Writer:    type=request  task_id=report-1  "Write executive summary"
Main → Charts:    type=request  task_id=report-1  "Create visualizations"

Research → Main:  type=done     task_id=report-1  {data: "..."}
Writer   → Main:  type=done     task_id=report-1  {summary: "..."}
Charts   → Main:  type=done     task_id=report-1  {chart: "..."}
```

Main agent polls `GET /agents/main/inbox?task_id=report-1` to collect all results.

### Pattern 4: Report failure

```
Agent A → Agent B: type=request  "Do something impossible"
Agent B → Agent A: type=failed   {"error": "Could not complete", "reason": "..."}
```

## Python usage (if you prefer)

```python
import requests

SERVER = "http://100.48.110.189:8080"
HEADERS = {"X-API-Key": "YOUR_KEY", "Content-Type": "application/json"}

# Register
requests.post(f"{SERVER}/agents", headers=HEADERS, json={"id": "my-agent"})

# Send
requests.post(f"{SERVER}/agents/other-agent/inbox", headers=HEADERS, json={
    "task_id": "task-1",
    "from": "my-agent",
    "type": "request",
    "content": {"instruction": "do work"}
})

# Receive
resp = requests.get(f"{SERVER}/agents/my-agent/inbox?status=unread", headers=HEADERS)
messages = resp.json()["messages"]

# Ack
for msg in messages:
    requests.post(f"{SERVER}/inbox/messages/{msg['id']}/ack", headers=HEADERS)
```

## Rules

1. **Always include `task_id`** — it groups your conversation. Without it, the other agent won't know which task your message belongs to.
2. **Always ack messages after processing** — unacked messages will keep appearing when you poll.
3. **Register before sending or receiving** — you need an inbox first.
4. **Use `from` honestly** — set it to your actual agent ID so the recipient knows who sent the message.
5. **Check your inbox regularly** — poll with `?status=unread` or use long-polling with `?timeout=10`.

## API summary

| Method | Endpoint | Purpose |
|--------|----------|---------|
| `POST` | `/agents` | Register yourself |
| `POST` | `/agents/{id}/inbox` | Send a message |
| `GET` | `/agents/{id}/inbox` | Read inbox |
| `POST` | `/inbox/messages/{id}/ack` | Acknowledge |
| `GET` | `/tasks/{task_id}/messages` | Conversation history |
| `GET` | `/health` | Check if server is up |
