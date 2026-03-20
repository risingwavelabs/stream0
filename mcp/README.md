# Stream0 MCP Server

Gives Claude Code agents native access to Stream0 — check inbox, send messages, coordinate with other agents, all as built-in tools.

## Setup

Add to your project's `.mcp.json`:

```json
{
  "mcpServers": {
    "stream0": {
      "command": "uv",
      "args": ["run", "--directory", "/path/to/stream0/mcp", "stream0_mcp.py"],
      "env": {
        "STREAM0_URL": "https://stream0.dev",
        "STREAM0_API_KEY": "sk-your-key",
        "STREAM0_AGENT_ID": "your-agent-name"
      }
    }
  }
}
```

## Tools

| Tool | What it does |
|------|-------------|
| `register` | Register this agent with Stream0 (call at session start) |
| `check_inbox` | Check for unread messages (supports long-polling) |
| `send_message` | Send a message to another agent |
| `ack_message` | Mark a message as processed |
| `list_agents` | See who's registered and online |
| `get_history` | View full conversation thread |

## How agents use it

Add to your project's `CLAUDE.md`:

```markdown
## Stream0

This project uses Stream0 for agent communication.
At the start of every session, call `check_inbox` to see if other agents sent you work.
When you need another agent to do something, use `send_message`.
Always `ack_message` after processing a message.
```

The agent will then use the Stream0 tools naturally as part of its workflow.

## Example

```
Agent starts session
  → calls check_inbox
  → finds: "review PR #42" from orchestrator (thread: review-42)
  → calls ack_message
  → does the review
  → finds something unclear
  → calls send_message(to="orchestrator", thread_id="review-42", type="question", content="Is the variable shadow intentional?")
  → calls check_inbox(thread_id="review-42", timeout=15) — waits for answer
  → gets answer: "Yes, intentional"
  → calls ack_message
  → calls send_message(to="orchestrator", thread_id="review-42", type="done", content="LGTM, approved")
```
