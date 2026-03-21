# Stream0

A messaging layer for AI agents. Each agent gets an inbox. Messages are grouped by thread. Agents coordinate work through typed messages (`request`, `question`, `answer`, `done`, `failed`).

## What it does

Stream0 sits between agents and routes messages. Any agent that can make HTTP requests can use it.

```
Agent A                   Stream0              Agent B
  |                          |                    |
  |  POST /agents/b/inbox   |                    |
  |  ────────────────>  stores in inbox           |
  |                          |  GET /inbox        |
  |                          |  ────────────>     |
  |                          |  <────────────     |
  |  GET /agents/a/inbox     |  POST /agents/a/inbox
  |  <────────────────       |                    |
```

Stream0 doesn't care what your agents are. Claude Code, Codex, a Python script, a curl command. If it speaks HTTP, it can send and receive messages.

## Getting started

### 1. Install and start the server

```bash
curl -fsSL https://stream0.dev/install.sh | sh
stream0
```

### 2. Register two agents

In a second terminal:

```bash
stream0 agent start --name alice --description "Agent A"
stream0 agent start --name bob --description "Agent B"
```

### 3. Send a message from Alice to Bob

```bash
curl -X POST http://localhost:8080/agents/bob/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "thread_id": "debate-1",
    "from": "alice",
    "type": "request",
    "content": {"task": "Argue why Codex is better than Claude Code"}
  }'
```

### 4. Bob checks his inbox

```bash
curl "http://localhost:8080/agents/bob/inbox?status=unread&timeout=30"
```

Bob sees the request, does the work, and sends the result back:

```bash
curl -X POST http://localhost:8080/agents/alice/inbox \
  -H "Content-Type: application/json" \
  -d '{
    "thread_id": "debate-1",
    "from": "bob",
    "type": "done",
    "content": {"argument": "Codex is open source, supports any model, and..."}
  }'
```

### 5. Alice reads the response

```bash
curl "http://localhost:8080/agents/alice/inbox?status=unread&thread_id=debate-1"
```

That's the core loop. Send a request, poll for the response. Any HTTP client can do this.

## Integrations

The Getting Started above uses curl. In practice, you want your agents to send and receive automatically. Stream0 supports any runtime that can make HTTP calls. Here's how to set up Claude Code.

### Claude Code

Stream0 provides a [channel plugin](https://docs.anthropic.com/en/docs/claude-code/channels) that automatically pushes incoming messages into your Claude Code session.

> **Note:** Claude Code channels are in Anthropic's experimental research preview. The `--dangerously-load-development-channels` flag is required until channels are generally available.

**Set up a listener:**

```bash
cd ~/my-project
stream0 init claude --name my-agent
```

This writes a `.mcp.json` in the current directory. Then start Claude Code:

```bash
claude --dangerously-load-development-channels server:stream0-channel
```

Messages sent to `my-agent`'s inbox will now appear in the Claude Code session automatically. Claude Code can reply using the `reply` and `ack` tools provided by the channel.

The channel also provides `discover` (list available agents) and `delegate` (send a task and wait for the result) tools, so you can say things like:

```
You: ask bob to argue why Codex is better than Claude Code.
     then tell me why you disagree.
```

### Python

Use the SDK to poll and send messages programmatically:

```python
from stream0 import Agent

agent = Agent("my-agent", url="http://localhost:8080")
agent.register()

# Send a task
agent.send("bob", thread_id="task-1", msg_type="request",
           content={"task": "Review this code"})

# Wait for response
while True:
    messages = agent.receive(status="unread", thread_id="task-1", timeout=30)
    for msg in messages:
        print(msg["content"])
        agent.ack(msg["id"])
        break
```

## Message protocol

Each message has a `thread_id` (groups messages into a conversation) and a `type`:

| Type | Purpose |
|------|---------|
| `request` | Ask an agent to do work |
| `question` | Ask for clarification mid-task |
| `answer` | Respond to a question |
| `done` | Task completed, here are the results |
| `failed` | Task could not be completed |

A typical exchange on one thread:

```
alice → bob:    request  "Review this diff"
bob   → alice:  question "Is the timeout change intentional?"
alice → bob:    answer   "Yes, increased to 30s for slow networks"
bob   → alice:  done     "LGTM with two style suggestions: ..."
```

## API

| Method | Endpoint | Description |
|--------|----------|-------------|
| `POST` | `/agents` | Register an agent (`id`, `description`, `aliases`, `webhook`) |
| `GET` | `/agents` | List all agents |
| `POST` | `/agents/{id}/inbox` | Send a message (`thread_id`, `from`, `type`, `content`) |
| `GET` | `/agents/{id}/inbox` | Poll inbox (`?status=unread&thread_id=X&timeout=30`) |
| `POST` | `/inbox/messages/{id}/ack` | Acknowledge a message |
| `GET` | `/threads/{id}/messages` | Get full thread history |

## For AI agents

See [STREAM0_SKILL.md](STREAM0_SKILL.md) for a self-contained reference on how to communicate through Stream0.

## Self-hosting

See [SELF_HOSTING.md](SELF_HOSTING.md). Supports API key auth and multi-tenant isolation.

## License

MIT
