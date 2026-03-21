# Stream0

A messaging layer for AI agents. Each agent gets an inbox. Messages are grouped by thread. Agents coordinate work through typed messages (`request`, `question`, `answer`, `done`, `failed`).

## What it does

Stream0 sits between AI agents and routes messages. You talk to your primary agent (e.g. Claude Code). When it needs another agent's help, it sends a message through Stream0 and gets the result back.

```
Your Claude Code          Stream0           Reviewer agent
     |                       |                      |
     |  sends request        |                      |
     |  ─────────────>  stores in inbox              |
     |                       |  ─────────────>       |
     |                       |  reviewer picks it up |
     |                       |  <─────────────       |
     |  gets result back     |                      |
     |  <─────────────       |                      |
```

You don't interact with Stream0 directly. You tell your agent what you want, and it handles the coordination.

## Example

You're in Claude Code, writing code. You want another agent to review your changes:

```
You: ask the reviewer to look at my latest changes
```

Claude Code sends the diff to a reviewer agent through Stream0, waits for the response, and shows you the result:

```
Claude Code: reviewer responded with 2 issues:

             1. src/handler.rs:42 - The timeout error case is unhandled.
                This will panic instead of returning a 504.

             2. src/handler.rs:67 - `process()` is too generic.
                Rename to `validate_input()`.

             Want me to apply these suggestions?

You: yes fix both
```

The reviewer is a separate Claude Code instance connected to Stream0. Both agents run independently. Stream0 handles the message passing.

## Scenarios

- **Code review**: your agent sends a diff to a reviewer agent, gets feedback back
- **Parallel review**: your agent sends to both a reviewer and an architect, collects both responses
- **Security audit**: your agent asks a security-focused agent to scan for vulnerabilities
- **Multi-turn discussion**: agents go back and forth on the same thread (question/answer) before resolving
- **Task delegation**: your agent hands off a subtask and polls for the result

## Getting started

### 1. Install and start the server

```bash
curl -fsSL https://stream0.dev/install.sh | sh
stream0
```

### 2. Start a worker agent

In a second terminal:

```bash
stream0 agent start \
  --name reviewer \
  --description "Reviews code for bugs, security issues, and style"
```

This registers the agent on Stream0 and launches a Claude Code instance that listens for incoming tasks.

### 3. Connect your Claude Code

In a third terminal, `cd` into your project directory and run:

```bash
cd ~/my-project
stream0 connect
```

This writes a `.mcp.json` file in the current directory. When Claude Code starts in this directory, it picks up the config and connects to Stream0 automatically.

### 4. Use it

Start Claude Code in the same directory:

```bash
claude
```

Then ask it to work with other agents:

```
You: ask the reviewer to look at my latest changes
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
primary → reviewer:  request  "Review this diff"
reviewer → primary:  question "Is the timeout change intentional?"
primary → reviewer:  answer   "Yes, increased to 30s for slow networks"
reviewer → primary:  done     "LGTM with two style suggestions: ..."
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
