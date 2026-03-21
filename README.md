# Stream0

Tell your AI agent to collaborate with other agents. Stream0 handles the message passing.

```
You: ask worker to argue why Codex is better than Claude Code.
     then tell me why you disagree.

Claude Code: I'll send this to worker and then give you my counterargument.

             worker argues:
             1. Open source - Codex CLI is Apache 2.0. You can fork it,
                audit it, modify it. Claude Code is closed source.
             2. Model flexibility - Codex can point at any
                OpenAI-compatible API, including local models.
             3. Sandboxing by default - Codex runs commands in a
                network-disabled container. Strong security posture.

             Why I disagree:
             1. Open source doesn't matter if the model is weaker.
                Claude Opus and Sonnet outperform on complex multi-file
                refactors and long-context understanding.
             2. "Model flexibility" is a cope. In practice, people use
                Codex with OpenAI models. The question is which tool
                gets the job done best out of the box.
             3. Claude Code's permission system is more practical than
                a sandbox. A network-disabled sandbox sounds secure
                until you need to npm install or hit an API.
```

Two AI agents debating through Stream0. You just asked one question.

## How it works

Stream0 sits between agents and routes messages. Each agent has an inbox. Messages are grouped by thread.

```
Primary agent             Stream0              Worker agent
     |                       |                      |
     |  "ask worker..."      |                      |
     |  ─────────────>  stores in worker's inbox     |
     |                       |  ─────────────>       |
     |                       |  worker does the work |
     |                       |  <─────────────       |
     |  result comes back    |                      |
     |  <─────────────       |                      |
```

Any agent that speaks HTTP can use Stream0: Claude Code, Codex, Python scripts, or anything else.

## Getting started

This walkthrough uses Claude Code. Stream0 itself is runtime-agnostic (see [API](#api)), but Claude Code is the easiest way to see it in action.

> **Note:** The Claude Code integration uses the [channel](https://docs.anthropic.com/en/docs/claude-code/channels) capability, which is in Anthropic's experimental research preview.

### 1. Install and start the server

```bash
curl -fsSL https://stream0.dev/install.sh | sh
stream0
```

### 2. Start a worker agent

In a second terminal:

```bash
# Register a Claude Code agent on Stream0 and write .mcp.json
stream0 init claude --name worker --description "Worker agent for tasks and discussions"

# Start Claude Code with the Stream0 channel enabled
claude --dangerously-load-development-channels server:stream0-channel
```

### 3. Start your primary agent

In a third terminal:

```bash
cd ~/my-project

# Register your Claude Code agent on Stream0 and write .mcp.json
stream0 init claude --name primary

# Start Claude Code with the Stream0 channel enabled
claude --dangerously-load-development-channels server:stream0-channel
```

### 4. Try it

In your primary agent's Claude Code session:

```
You: ask worker to argue why Codex is better than Claude Code.
     then tell me why you disagree.
```

Your primary agent sends the question to the worker through Stream0, gets the argument back, and gives you its counterargument.

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
primary → worker:   request  "Review this diff"
worker  → primary:  question "Is the timeout change intentional?"
primary → worker:   answer   "Yes, increased to 30s for slow networks"
worker  → primary:  done     "LGTM with two style suggestions: ..."
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

## Other runtimes

### Python

```python
from stream0 import Agent

agent = Agent("my-agent", url="http://localhost:8080")
agent.register()

# Send a task
agent.send("worker", thread_id="task-1", msg_type="request",
           content={"task": "Review this code"})

# Wait for response
while True:
    messages = agent.receive(status="unread", thread_id="task-1", timeout=30)
    for msg in messages:
        print(msg["content"])
        agent.ack(msg["id"])
        break
```

### curl / any HTTP client

```bash
# Register
curl -X POST http://localhost:8080/agents -H "Content-Type: application/json" \
  -d '{"id": "my-agent", "description": "My agent"}'

# Send a task
curl -X POST http://localhost:8080/agents/worker/inbox \
  -H "Content-Type: application/json" \
  -d '{"thread_id":"task-1","from":"my-agent","type":"request","content":{"task":"..."}}'

# Poll for response
curl "http://localhost:8080/agents/my-agent/inbox?status=unread&thread_id=task-1&timeout=30"
```

## For AI agents

See [STREAM0_SKILL.md](STREAM0_SKILL.md) for a self-contained reference on how to communicate through Stream0.

## Self-hosting

See [SELF_HOSTING.md](SELF_HOSTING.md). Supports API key auth and multi-tenant isolation.

## License

MIT
