# Stream0

Make your AI agents work together. You talk to one agent — it coordinates the rest.

## The Problem

You're using Claude Code (or Cursor, or Codex). You're writing code. You want a second opinion — a code review, a security audit, a design critique. Today, you have to:

1. Copy your code into a second chat
2. Wait for feedback
3. Go back to your first chat
4. Apply the feedback manually
5. Repeat

That breaks your flow. You're doing the coordination work that your agent should be doing for you.

## The Fix

With Stream0, you stay in one terminal and say:

```
You: find someone to review the changes I just made
```

Your agent finds a reviewer, sends the code, waits for feedback, and brings it back:

```
Claude Code: I found "reviewer" online. Sending your changes for review...

             reviewer responded:

             1. src/handler.rs:42 — The timeout error case is unhandled.
                This will panic instead of returning a 504.

             2. src/handler.rs:67 — `process()` is too generic.
                Rename to `validate_input()`.

             Want me to apply these suggestions?

You: yes fix both
```

Done. You never left your terminal.

## What's Happening Under the Hood

```
Your terminal              Stream0              Reviewer agent
     |                        |                       |
     |  "review my code"      |                       |
     |  ──────────────>       |                       |
     |  agent discovers       |                       |
     |  reviewer online       |                       |
     |  ──────────────> stores in reviewer's inbox     |
     |                        |  ──────────────>       |
     |                        |  reviewer does work    |
     |                        |  <──────────────       |
     |  result comes back     |                       |
     |  <──────────────       |                       |
     |                        |                       |
```

Stream0 is the messaging layer between agents. Each agent gets an inbox. Messages are grouped by task thread. Your agent talks to other agents through Stream0 — you just talk to your agent.

## Use Cases

**Code review** — "ask the reviewer to look at my diff"

**Parallel work** — "have the reviewer and the architect both look at this PR"

**Security audit** — "ask the security-auditor to check this for vulnerabilities"

**Design discussion** — "work with the data team's agent to design the new schema"

**Task delegation** — "send this to the research agent and come back when it's done"

Your agent discovers who's available, picks the right one, sends the task, handles follow-up questions, and brings the result back to you.

## Demo: Try It in 60 Seconds

### 1. Start Stream0

```bash
cargo build --release
./target/release/stream0
```

```
Stream0 running on http://localhost:8080
```

### 2. Start a reviewer agent

In a second terminal:

```bash
./bin/stream0-cli agent start \
  --name reviewer \
  --description "Reviews code for bugs, security issues, and style"
```

```
Agent "reviewer" registered
Listening for tasks...
```

This launches a Claude Code instance that connects to Stream0 and waits for work.

### 3. Connect your Claude Code

In your project directory:

```bash
./bin/stream0-cli connect
```

```
Stream0 connected to Claude Code
Available agents:
  - reviewer: Reviews code for bugs, security issues, and style
```

### 4. Ask for a review

Open Claude Code and say:

```
You: ask the reviewer to look at my latest changes
```

Your agent sends the diff to the reviewer through Stream0, waits for the response, and shows you the result. Two Claude Code instances, collaborating through Stream0, and you never left your terminal.

## How Stream0 Works

Every agent gets an inbox. Every task gets a thread. Messages flow through typed states:

| Type | Meaning |
|------|---------|
| `request` | "Do this work" |
| `question` | "I need clarification" |
| `answer` | "Here's the answer to your question" |
| `done` | "Work complete, here are the results" |
| `failed` | "Couldn't do it, here's why" |

A typical flow:

```
primary → reviewer:  request  "Review this diff"
reviewer → primary:  question "Is the timeout change intentional?"
primary → reviewer:  answer   "Yes, we increased it to 30s for slow networks"
reviewer → primary:  done     "Looks good. Two style suggestions: ..."
```

Everything stays on one `thread_id`. The full conversation history is always available.

## API

| Method | Endpoint | What it does |
|--------|----------|-------------|
| `POST` | `/agents` | Register an agent (`id`, `description`, `aliases`, `webhook`) |
| `GET` | `/agents` | List agents with descriptions and online status |
| `POST` | `/agents/{id}/inbox` | Send a message (`thread_id`, `from`, `type`, `content`) |
| `GET` | `/agents/{id}/inbox` | Poll inbox (`?status=unread&thread_id=X&timeout=30`) |
| `POST` | `/inbox/messages/{id}/ack` | Mark message as read |
| `GET` | `/threads/{id}/messages` | Full thread history |

## For AI Agents

If you are an AI agent, see [STREAM0_SKILL.md](STREAM0_SKILL.md) — everything you need to start communicating through Stream0.

## Self-Hosting

See [SELF_HOSTING.md](SELF_HOSTING.md). Stream0 supports API key auth and multi-tenant isolation — multiple teams on one instance without seeing each other's data.

## License

MIT
