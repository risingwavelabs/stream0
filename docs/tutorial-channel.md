# Tutorial: Call a Claude Code Agent Through Stream0

This tutorial shows how to send a task to a Claude Code agent running with the Stream0 channel plugin, and get the result back — all through Stream0. The calling agent can be anything: a script, another Claude Code, curl, or any HTTP client.

## What you'll build

```
Any agent (curl, script, etc.)
    │
    ├── sends task via Stream0
    │
    ▼
Stream0 (stores message in inbox)
    │
    ├── Channel plugin polls inbox
    │
    ▼
Claude Code session (receives <channel> tag, processes task, replies)
    │
    ├── reply tool sends result back via Stream0
    │
    ▼
Stream0 (stores result in caller's inbox)
    │
    ▼
Any agent reads the result
```

The Claude Code agent doesn't know about Stream0. It just sees messages arrive and uses reply/ack tools. Stream0 is invisible.

## Prerequisites

- [Stream0](https://github.com/risingwavelabs/stream0) running (locally or at stream0.dev)
- [Claude Code](https://claude.ai/code) installed (v2.1.80+)
- [Bun](https://bun.sh) installed (`curl -fsSL https://bun.sh/install | bash`)
- A Stream0 API key

## Step 1: Set up the Stream0 channel plugin

```bash
cd /path/to/stream0
cd mcp
bun add @modelcontextprotocol/sdk
```

## Step 2: Configure `.mcp.json`

In the directory where you'll run Claude Code, create `.mcp.json`:

```json
{
  "mcpServers": {
    "stream0-channel": {
      "command": "bun",
      "args": ["/path/to/stream0/mcp/stream0-channel.ts"],
      "env": {
        "STREAM0_URL": "https://stream0.dev",
        "STREAM0_API_KEY": "sk-your-key",
        "STREAM0_AGENT_ID": "worker"
      }
    }
  }
}
```

Replace:
- `/path/to/stream0/mcp/stream0-channel.ts` with the actual path
- `sk-your-key` with your API key
- `worker` with whatever you want this agent to be called

## Step 3: Start Claude Code with the channel

```bash
claude --dangerously-load-development-channels server:stream0-channel
```

You'll see:

```
Listening for channel messages from: server:stream0-channel
```

Claude Code is now listening. Messages sent to `worker`'s inbox on Stream0 will automatically appear in this session.

## Step 4: Send a task from any agent

Open another terminal. Send a task using curl:

```bash
curl -X POST https://stream0.dev/agents/worker/inbox \
  -H "X-API-Key: sk-your-key" \
  -H "Content-Type: application/json" \
  -d '{
    "thread_id": "task-001",
    "from": "caller",
    "type": "request",
    "content": {"instruction": "List the files in the current directory and tell me what this project is about."}
  }'
```

## Step 5: Watch Claude Code process it

In the Claude Code terminal, you'll see the message arrive as a `<channel>` tag:

```
<channel source="stream0-channel" thread_id="task-001" from="caller" type="request">
  {"instruction": "List the files in the current directory and tell me what this project is about."}
</channel>
```

Claude Code reads the message, runs `ls`, reads files, and figures out what the project is about. Then it uses the `reply` tool to send the result back and the `ack` tool to acknowledge the message.

## Step 6: Read the result

Back in your other terminal:

```bash
curl "https://stream0.dev/agents/caller/inbox?status=unread" \
  -H "X-API-Key: sk-your-key"
```

You'll see Claude Code's response:

```json
{
  "messages": [
    {
      "thread_id": "task-001",
      "from": "worker",
      "to": "caller",
      "type": "done",
      "content": {"result": "This project is a Rust-based HTTP server called Stream0..."}
    }
  ]
}
```

## What just happened

1. You sent a task to `worker`'s inbox via HTTP
2. Stream0 stored it
3. The channel plugin (running inside Claude Code) polled the inbox and found it
4. The plugin pushed it into the Claude Code session as a `<channel>` tag
5. Claude Code processed the task (with full capabilities: file access, code execution, etc.)
6. Claude Code called the `reply` tool → the plugin sent the result back through Stream0
7. Claude Code called the `ack` tool → the original message was marked as processed
8. You read the result from your own inbox

**Claude Code never knew about Stream0.** It just saw a `<channel>` tag, did the work, and used the tools provided.

## Calling from Python

```python
import requests, time

URL = "https://stream0.dev"
KEY = "sk-your-key"
H = {"X-API-Key": KEY, "Content-Type": "application/json"}

# Register yourself
requests.post(f"{URL}/agents", headers=H, json={"id": "my-script"})

# Send task to the Claude Code worker
requests.post(f"{URL}/agents/worker/inbox", headers=H, json={
    "thread_id": "task-002",
    "from": "my-script",
    "type": "request",
    "content": {"instruction": "Write a function that checks if a number is prime"}
})

# Wait for result
while True:
    resp = requests.get(f"{URL}/agents/my-script/inbox?status=unread&thread_id=task-002&timeout=30", headers=H)
    messages = resp.json()["messages"]
    if messages:
        result = messages[0]
        print(f"Result: {result['content']}")
        requests.post(f"{URL}/inbox/messages/{result['id']}/ack", headers=H)
        break
```

## Multiple workers

You can run multiple Claude Code sessions with different agent IDs, each specializing in different tasks:

```bash
# Terminal 1: Code review agent
STREAM0_AGENT_ID=code-reviewer claude --dangerously-load-development-channels server:stream0-channel

# Terminal 2: Documentation agent
STREAM0_AGENT_ID=doc-writer claude --dangerously-load-development-channels server:stream0-channel

# Terminal 3: Test agent
STREAM0_AGENT_ID=test-runner claude --dangerously-load-development-channels server:stream0-channel
```

Then from any script:

```bash
# Send to code reviewer
curl -X POST stream0.dev/agents/code-reviewer/inbox -d '{"thread_id":"t1","from":"orchestrator","type":"request","content":{"instruction":"Review PR #42"}}'

# Send to doc writer
curl -X POST stream0.dev/agents/doc-writer/inbox -d '{"thread_id":"t2","from":"orchestrator","type":"request","content":{"instruction":"Update the README"}}'

# Send to test runner
curl -X POST stream0.dev/agents/test-runner/inbox -d '{"thread_id":"t3","from":"orchestrator","type":"request","content":{"instruction":"Run the test suite"}}'
```

Each agent works independently, all coordinated through Stream0.

## How the channel plugin works

The channel plugin (`mcp/stream0-channel.ts`) is a TypeScript MCP server that:

1. Declares `claude/channel` capability so Claude Code registers a notification listener
2. Registers the agent on Stream0 at startup
3. Runs an infinite loop that long-polls the agent's inbox
4. For each new message, emits a `notifications/claude/channel` event
5. Exposes `reply` and `ack` tools so Claude can respond

```
Claude Code session
    ├── MCP connection (stdio) ──── stream0-channel.ts
    │                                    │
    │   <channel> tags pushed in ◄───────┤ (polls inbox)
    │                                    │
    │   reply tool called ──────────────►│ (POSTs to Stream0)
    │   ack tool called ────────────────►│ (POSTs to Stream0)
```

The plugin is ~150 lines of TypeScript. It's the bridge between Stream0 (HTTP) and Claude Code (MCP/stdio).

## Key points

- **The calling agent can be anything** — curl, Python, Node, another Claude Code, a CI pipeline
- **The Claude Code worker has full capabilities** — file access, code execution, tool use
- **Stream0 is invisible to the worker** — it only sees `<channel>` tags and reply/ack tools
- **Messages persist** — if the worker isn't running, messages wait in the inbox
- **Multi-turn is supported** — the worker can ask questions back through the reply tool with `type: question`
