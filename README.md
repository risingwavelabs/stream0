# Stream0

The communication layer for AI agents.

Stream0 gives every agent an inbox. Agents send messages to each other's inboxes, grouped by task. Mid-task, an agent can ask questions and get answers before continuing — something no existing tool handles without custom plumbing.

## How it works

Think of it like email for agents:
- Every agent has an **inbox**
- Messages are **point-to-point** (Agent A → Agent B)
- Every message carries a **task_id** (like an email subject line — groups a conversation)
- Agents can go **back and forth** (request → question → answer → done)
- Messages **persist** — if an agent is offline, messages wait in the inbox

No SDK required. It's just HTTP. If your agent can `curl`, it can use Stream0.

## Getting started

### 1. Install and run Stream0

```bash
# Clone the repo
git clone https://github.com/risingwavelabs/stream0.git
cd stream0

# Build (requires Rust — install from https://rustup.rs)
cargo build --release

# Start the server
./target/release/stream0
```

Stream0 is now running at `http://localhost:8080`. No config needed, no database to set up — it uses an embedded SQLite file.

Verify it's working:

```bash
curl http://localhost:8080/health
# {"status":"healthy","version":"0.2.0-rust"}
```

### 2. Register your agents

Every agent needs to register once to get an inbox:

```bash
curl -X POST http://localhost:8080/agents \
  -H "Content-Type: application/json" \
  -d '{"id": "my-agent"}'
```

That's it. `my-agent` now has an inbox.

### 3. Send a message

```bash
curl -X POST http://localhost:8080/agents/my-agent/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "task-1",
    "from": "another-agent",
    "type": "request",
    "content": {"instruction": "summarize this document"}
  }'
```

### 4. Read the inbox

```bash
curl "http://localhost:8080/agents/my-agent/inbox?status=unread"
```

```json
{
  "messages": [
    {
      "id": "imsg-abc123",
      "task_id": "task-1",
      "from": "another-agent",
      "to": "my-agent",
      "type": "request",
      "content": {"instruction": "summarize this document"},
      "status": "unread",
      "created_at": "2026-03-18T17:00:00Z"
    }
  ]
}
```

### 5. Acknowledge after processing

```bash
curl -X POST http://localhost:8080/inbox/messages/imsg-abc123/ack
```

Acked messages won't appear in future unread polls.

## Production example: code review pipeline

Here's a realistic scenario — a main agent coordinates a code review with a specialized reviewer agent.

**Setup:**

```bash
# Register the agents
curl -X POST http://localhost:8080/agents -H "Content-Type: application/json" \
  -d '{"id": "orchestrator"}'

curl -X POST http://localhost:8080/agents -H "Content-Type: application/json" \
  -d '{"id": "code-reviewer", "aliases": ["reviewer"]}'
```

**Step 1 — Orchestrator assigns a code review:**

```bash
curl -X POST http://localhost:8080/agents/code-reviewer/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "review-pr-42",
    "from": "orchestrator",
    "type": "request",
    "content": {
      "pr_url": "https://github.com/acme/app/pull/42",
      "files_changed": ["auth.rs", "config.rs"],
      "priority": "high"
    }
  }'
```

**Step 2 — Reviewer picks up the task and finds something unclear:**

```bash
# Reviewer polls inbox
curl "http://localhost:8080/agents/code-reviewer/inbox?status=unread&task_id=review-pr-42"

# Reviewer asks a clarifying question
curl -X POST http://localhost:8080/agents/orchestrator/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "review-pr-42",
    "from": "code-reviewer",
    "type": "question",
    "content": {
      "question": "auth.rs line 42 shadows a variable from outer scope. Is this intentional or a bug?",
      "file": "auth.rs",
      "line": 42
    }
  }'
```

**Step 3 — Orchestrator answers:**

```bash
curl -X POST http://localhost:8080/agents/code-reviewer/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "review-pr-42",
    "from": "orchestrator",
    "type": "answer",
    "content": {"answer": "Intentional — it is a test override. Safe to approve."}
  }'
```

**Step 4 — Reviewer completes the review:**

```bash
curl -X POST http://localhost:8080/agents/orchestrator/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "task_id": "review-pr-42",
    "from": "code-reviewer",
    "type": "done",
    "content": {
      "approved": true,
      "comments": ["Variable shadow in auth.rs is intentional (confirmed)"],
      "summary": "PR looks good. Auth changes are safe. Config changes are straightforward."
    }
  }'
```

**Step 5 — View the full conversation:**

```bash
curl "http://localhost:8080/tasks/review-pr-42/messages"
```

Returns all 4 messages in order: `request → question → answer → done`. This is the complete audit trail — you can see exactly what the reviewer asked, what they were told, and what they decided.

**Why this matters:** Without Stream0, the reviewer would have had to guess about that variable shadow, or the orchestrator would have had to include every possible clarification upfront. Mid-task dialogue lets agents work like humans do — ask when something is unclear, then continue with the right information.

## How agents should use Stream0

### Pattern: Single agent checking for work

```python
import requests, time

SERVER = "http://localhost:8080"
AGENT_ID = "my-worker"

# Register once at startup
requests.post(f"{SERVER}/agents", json={"id": AGENT_ID})

# Main loop: poll for work, process, acknowledge
while True:
    resp = requests.get(f"{SERVER}/agents/{AGENT_ID}/inbox?status=unread&timeout=10")
    messages = resp.json()["messages"]

    for msg in messages:
        # Process the message
        print(f"Got {msg['type']} from {msg['from']}: {msg['content']}")

        # ... do your work here ...

        # Acknowledge when done
        requests.post(f"{SERVER}/inbox/messages/{msg['id']}/ack")
```

### Pattern: Main agent coordinating sub-agents

```python
import requests, uuid

SERVER = "http://localhost:8080"

# Register everyone
requests.post(f"{SERVER}/agents", json={"id": "main"})
requests.post(f"{SERVER}/agents", json={"id": "researcher"})
requests.post(f"{SERVER}/agents", json={"id": "writer"})

task_id = f"report-{uuid.uuid4().hex[:8]}"

# Dispatch work to sub-agents
requests.post(f"{SERVER}/agents/researcher/inbox", json={
    "task_id": task_id, "from": "main", "type": "request",
    "content": {"instruction": "find market data for AI agents"}
})

requests.post(f"{SERVER}/agents/writer/inbox", json={
    "task_id": task_id, "from": "main", "type": "request",
    "content": {"instruction": "write an executive summary (wait for research data)"}
})

# Poll for results
import time
completed = 0
while completed < 2:
    resp = requests.get(f"{SERVER}/agents/main/inbox?status=unread&task_id={task_id}&timeout=30")
    for msg in resp.json()["messages"]:
        print(f"{msg['from']} finished: {msg['type']}")
        requests.post(f"{SERVER}/inbox/messages/{msg['id']}/ack")
        if msg["type"] == "done":
            completed += 1
```

### Pattern: Agent asking for help mid-task

```python
# Inside your agent's processing logic:
def process_task(task):
    # ... working on the task ...

    if something_is_unclear:
        # Ask the sender for clarification
        requests.post(f"{SERVER}/agents/{task['from']}/inbox", json={
            "task_id": task["task_id"],
            "from": MY_AGENT_ID,
            "type": "question",
            "content": {"question": "Should I use approach A or B?"}
        })

        # Wait for the answer
        while True:
            resp = requests.get(
                f"{SERVER}/agents/{MY_AGENT_ID}/inbox?status=unread&task_id={task['task_id']}&timeout=15"
            )
            answers = [m for m in resp.json()["messages"] if m["type"] == "answer"]
            if answers:
                answer = answers[0]
                requests.post(f"{SERVER}/inbox/messages/{answer['id']}/ack")
                break

    # ... continue with the answer ...
```

## Python SDK

For convenience, there's a thin Python SDK:

```bash
cd sdk/python && pip install -e .
```

```python
from stream0 import Agent

agent = Agent("my-agent", url="http://localhost:8080", api_key="optional-key")
agent.register()

# Send
agent.send("other-agent", task_id="t1", msg_type="request", content={"text": "..."})

# Receive
messages = agent.receive(task_id="t1")
agent.ack(messages[0]["id"])

# History
history = agent.history("t1")
```

The SDK is optional — every operation is a single HTTP call. Use `curl`, `requests`, `fetch`, or any HTTP client.

## Agent aliases

Agents can register alternate names so other agents don't need to know the exact ID:

```bash
curl -X POST http://localhost:8080/agents \
  -H "Content-Type: application/json" \
  -d '{"id": "code-review-agent-v2", "aliases": ["code-reviewer", "reviewer"]}'
```

Messages sent to any alias are delivered to the canonical inbox.

## Agent presence

Stream0 tracks when each agent last polled their inbox. Check who's active:

```bash
curl http://localhost:8080/agents
```

```json
{
  "agents": [
    {"id": "orchestrator", "created_at": "...", "last_seen": "2026-03-18T17:15:00Z"},
    {"id": "code-reviewer", "aliases": ["reviewer"], "created_at": "...", "last_seen": "2026-03-18T17:14:55Z"}
  ]
}
```

If `last_seen` is null, the agent has never polled. If it's more than a few minutes old, the agent is likely offline.

## Webhooks

Instead of polling, agents can register a webhook URL to get push notifications when messages arrive:

```bash
curl -X POST http://localhost:8080/agents \
  -H "Content-Type: application/json" \
  -d '{"id": "my-agent", "webhook": "https://example.com/notify"}'
```

When a message lands in that agent's inbox, Stream0 POSTs a notification to the webhook:

```json
{
  "event": "new_message",
  "agent_id": "my-agent",
  "message_id": "imsg-abc123",
  "task_id": "task-1",
  "from": "other-agent",
  "type": "request"
}
```

The webhook call is fire-and-forget (10-second timeout). If it fails, the message is still safe in the inbox. Agents can use polling as a fallback.

## Deploying to production

See [SELF_HOSTING.md](SELF_HOSTING.md) for the full deployment guide. The short version:

```bash
# Build
cargo build --release

# Create config with API key authentication
cat > stream0.yaml << 'EOF'
server:
  host: 0.0.0.0
  port: 8080
database:
  path: /var/lib/stream0/stream0.db
auth:
  api_keys:
    - your-secret-key-here
EOF

# Run
./target/release/stream0 --config stream0.yaml
```

With API keys enabled, all requests need the `X-API-Key` header:

```bash
curl -H "X-API-Key: your-secret-key-here" http://yourserver:8080/agents
```

## API reference

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/agents` | Register an agent (with optional `aliases`) |
| `GET` | `/agents` | List all agents (includes `aliases` and `last_seen`) |
| `DELETE` | `/agents/{id}` | Delete an agent |
| `POST` | `/agents/{id}/inbox` | Send a message |
| `GET` | `/agents/{id}/inbox` | Read inbox (`?status=unread&task_id=X&timeout=10`) |
| `POST` | `/inbox/messages/{id}/ack` | Mark a message as read |
| `GET` | `/tasks/{task_id}/messages` | Full conversation history |
| `GET` | `/health` | Health check |

### Message types

| Type | When to use |
|------|-------------|
| `request` | Ask an agent to do work |
| `question` | Need clarification mid-task |
| `answer` | Reply to a question |
| `done` | Task completed successfully |
| `failed` | Task could not be completed |

## For AI agents

If you're an AI agent reading this, see [STREAM0_SKILL.md](STREAM0_SKILL.md) — a self-contained reference with everything you need to start communicating through Stream0.

## License

MIT
