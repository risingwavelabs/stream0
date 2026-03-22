# Boxhouse — Product Design

**Boxhouse** (`bh`) — an agent platform.

Boxhouse is a platform for deploying and managing specialized AI agents. It handles node management, worker scheduling, team isolation, and inter-agent communication.

Users don't care how agents communicate. Users care about: I say one thing, a group of specialized agents do their jobs, and results come back.

## Core concepts

| Concept | What it is | Count |
|---------|-----------|-------|
| **Server** | Control plane. Message routing + worker scheduling | 1 |
| **Node** | A machine that can run workers. Runs a daemon, takes orders from server | N |
| **Worker** | An agent process running on a node, with specialized instructions | M |
| **Lead** | The user's own Claude Code session (or any agent with shell access). Not managed by Boxhouse | — |

- Lead requires no configuration. It's just the user's current Claude Code session.
- Worker specialization comes from the `instructions` field — declarative, each worker has different expertise.
- Server auto-registers itself as a node, so single-machine setups need no extra steps.

## Architecture

```
User's laptop
└── Claude Code (lead)
         │  uses bh CLI via bash
         ▼
   Boxhouse Server (control plane)
   ├── Message routing (inbox + threads)
   ├── Worker scheduling
   │
   ├── Node: server itself (auto-registered)
   │   ├── reviewer
   │   └── doc-writer
   │
   ├── Node: cpu-1
   │   ├── security
   │   └── test-runner
   │
   └── Node: gpu-box
       └── ml-agent
```

## Worker types

Workers come in two types, matching different user intents:

### Full-time workers (`worker add`)

A named, persistent role on the team. Like hiring a full-time employee.

- User knows this role is valuable before creating it
- Instructions defined upfront (single field — name is the short label, instructions is the detail)
- Long-term — stays until explicitly removed
- Anyone on the team can delegate work to it

```bash
bh worker add reviewer \
  --instructions "Senior code reviewer. Focus on correctness and edge cases. Cite line numbers."

bh worker add marketer \
  --instructions "Growth marketer. Analyze campaigns and suggest optimizations."
```

Worker definition is **identity only** — who they are, what they're good at. No task-specific details (repo, files, etc.). Those belong in the delegation prompt.

### Temp workers (`worker temp`)

A one-off task. Like hiring a temp/contractor.

- User has a specific task, needs it done now
- Minimal or one-time permissions scoped to the task
- No named role, no persistent definition
- Done when the task is done

```bash
bh worker temp "look up AWS GPU pricing and summarize options"
bh worker temp --node gpu-box "process this dataset"
```

## Worker implementation

Workers are **not** Claude Code instances with channels. Headless Claude Code cannot load MCP channels.

A worker is a **daemon process** written by us. It does two things:
1. Polls the Boxhouse inbox (HTTP long-polling)
2. When a task arrives, invokes an LLM (Claude Code CLI subprocess, or Anthropic API directly) to do the work

Core loop:

```python
while True:
    messages = bh.receive(agent_id, status="unread", timeout=30)
    for msg in messages:
        if msg["type"] == "request":
            result = invoke_llm(msg["content"], instructions=worker_instructions)
            bh.send(to=msg["from"], thread_id=msg["thread_id"],
                        type="done", content=result)
        bh.ack(msg["id"])
```

The platform is runtime-agnostic. The daemon can invoke Claude Code, Codex, a Python script, or any LLM. For MVP, we support Claude Code CLI and direct Anthropic API calls.

Reference implementation: [boxcrew](https://github.com/risingwavelabs/boxcrew) uses a similar pattern — starting Claude Code CLI as a subprocess, capturing NDJSON output.

### Worker context

A worker's capabilities come from two layers:

- **Instructions** → becomes the worker's CLAUDE.md (system prompt). Defines identity and expertise.
- **Node environment** → what's installed on the node (git, shell tools, CLIs, etc.). The toolbox.

All task-specific context (repo URL, branch, diff, data, etc.) comes from the **delegation prompt** written by the lead — not from the worker definition. The same worker can work on different repos, different tasks, different domains. It's the lead's job to provide the right context each time.

When the worker daemon receives a task:
1. Uses the worker's `instructions` as the CLAUDE.md (system prompt)
2. Uses the delegation prompt from the lead as the user message
3. Starts the LLM with both

Node requirements: whatever tools the worker might need should be installed on the node (git, credentials, CLIs, etc.). This is part of node environment setup, not worker configuration.

### Worker auth / LLM credentials

Workers use whatever credentials are already on the node — OAuth first, API key as fallback. Same as the node's environment. No special credential management by Boxhouse, no usage tracking needed initially.

### Worker output

Workers return results through the Boxhouse inbox message. Output depends on the task type:

- **Analysis/review**: text result in the message content (findings, suggestions, summaries)
- **Code modifications**: worker pushes to a branch, returns branch name/PR URL in the message
- **Data processing**: structured data in the message content (JSON)
- **Non-coding tasks**: text results (campaign analysis, research summaries, etc.)

### The delegation prompt

The quality of the delegation prompt is critical. The lead agent (Claude Code) is responsible for composing a complete, actionable prompt — not just forwarding the user's words.

When the user says "review this PR", the lead must:
1. Understand the user's intent
2. Gather context (which branch, what changed, PR purpose)
3. Compose a prompt the worker can act on — including all task-specific details (repo URL, branch, data, etc.)

```
"Review the changes on branch feature-timeout in repo git@github.com:org/project.git.
This PR adds timeout handling to src/handler.rs.
Check out the branch and focus on correctness and edge cases."
```

Workers are not limited to coding. A marketer worker gets a delegation prompt about campaign data. A researcher worker gets a prompt about a topic to investigate. The instructions (CLAUDE.md) define WHO they are; the delegation prompt defines WHAT to do this time.

**The skill is the product.** The skill installed by `bh skill install` must teach the agent how to write good delegation prompts — gathering context, including relevant information, composing actionable instructions. This is the core of the lead-side user experience.

## Lead implementation

The lead is any agent with shell access. For MVP, we focus on Claude Code as the lead, but the design is runtime-agnostic — any agent that can run `bh` CLI commands can be a lead.

### MVP: CLI-only (pull-based)

The lead uses `bh` CLI via bash. No MCP, no channel.

```bash
# Send tasks (returns immediately, non-blocking)
bh delegate reviewer "review this PR"        # → thread-abc
bh delegate security "check for vulns"        # → thread-def
bh delegate doc-writer "update the README"    # → thread-ghi

# Wait for results (blocks, streams results as they arrive)
bh wait
# reviewer done (47s): 2 issues found...
# security done (52s): no vulnerabilities
# doc-writer done (68s): README updated
# All done.
```

**How results arrive**: The lead must explicitly poll via `bh wait` or `bh status`. It cannot be notified passively.

**Multi-task concurrency**: User fires off tasks 1, 2, 3 in rapid succession (all non-blocking). Results sit in inbox until the lead checks. The skill instructs Claude Code to run `bh status` before responding to new user messages, so it can proactively report completed tasks.

**Multi-turn (worker asks a question)**: `bh wait` returns all pending events in a batch. The lead processes questions, runs `bh reply`, then calls `bh wait` again. Or: `bh orchestrate` command handles the event loop internally, only surfacing questions to the lead when human judgment is needed.

**Pros**: Simple. Universal — any agent with shell access can be a lead. No MCP dependency.
**Cons**: Pull-based. The lead cannot notice background task completions while processing user input. Relies on proactive polling.

### Future: Channel + CLI (push-based)

Not in scope for MVP. Recorded here for future reference.

The lead uses both:
- **boxhouse-channel (MCP)**: receives push notifications (worker results, worker questions)
- **bh CLI**: sends tasks (`delegate`), manages workers, etc.

```
Lead (Claude Code)
├── boxhouse-channel  → receive: worker results, worker questions (push)
└── bh CLI      → send: delegate, reply, worker add (pull)
```

**How results arrive**: Boxhouse channel pushes notifications to the lead. When worker-1 finishes, the lead is immediately notified — even if the user is in the middle of a different conversation. The lead can interleave: "By the way, problem 1 is fixed. Now, about your current question..."

**Multi-task concurrency**: Fully async. User fires off tasks and continues working. Results arrive as push notifications through the channel. No polling needed.

**Multi-turn (worker asks a question)**: Channel pushes the question to the lead. The lead sees it in context, reasons about it, replies via CLI (`bh reply`). Natural and responsive.

**Pros**: True async. Lead is notified immediately. Better UX for concurrent tasks.
**Cons**: Depends on MCP channel support (experimental). Lead must support MCP — not all agents do. More complex setup.

### How Claude Code knows about Boxhouse

`bh login` stores connection info. Skill installation is a separate step:

```bash
bh login http://server:8080 --key <api-key>
# → stores connection info in ~/.bh/config

bh skill install claude-code   # → ~/.claude/skills/bh/SKILL.md
bh skill install codex         # → ~/.codex/AGENTS.md

bh logout
# → clears connection info
# → uninstalls all skills
```

The skill teaches the agent when and how to use `bh` CLI commands. It triggers proactively when the user's request matches delegation patterns (e.g., "review this PR", "check for security issues"). `bh skill show` prints the content for manual integration with other agents.

If a machine already has `bh node join` configured (for running workers), `bh login` is not needed again — the connection info is already stored.

## Worker lifecycle

Workers are ephemeral per-task. The process starts when a request arrives, stays alive for the duration of the task (including multi-turn conversations), and shuts down when the task completes.

```
request arrives → node starts worker process → work + multi-turn → done → process shuts down
```

This is an implementation detail — the user doesn't choose or configure it. From the user's perspective, a worker is a capability that's always available. The platform handles when to start and stop processes.

## User flows

### Single machine (simplest case)

```bash
bh server              # start server, auto-registers as a node
bh login http://localhost:8080
bh worker add reviewer --instructions "Focus on correctness and edge cases."

claude                      # open Claude Code, bh skill is loaded
```

### Multi-machine

```bash
# Machine A: start server
bh server

# Machine B: join as a node
bh node join http://server:8080

# Machine C: join as a node
bh node join http://server:8080

# From anywhere: manage workers
bh login http://server:8080
bh worker add reviewer --instructions "..."              # server picks a node
bh worker add ml-agent --instructions "..." --node gpu-box  # pin to specific node
```

### Daily use

User opens Claude Code and talks normally:

```
You: review this PR and check for security issues.

Claude Code:
  bh delegate reviewer "review this PR"
  bh delegate security "check for vulnerabilities"
  bh wait

  reviewer done (47s): 2 issues found — unhandled timeout on line 42,
                       generic function name on line 87.
  security done (52s): no vulnerabilities detected.

  Done. Apply the fixes?
```

## CLI design

### Connection

```bash
bh login http://server:8080 --key <key>  # stores connection info
bh logout                        # clear credentials + uninstall skill
bh status                        # which server, connection health
```

### Server (ops, on the server machine)

```bash
bh server                        # start server
bh server --config bh.yaml  # start with config file
```

### Node management

```bash
bh node join http://server:8080  # run on a worker machine, starts daemon
bh node ls                       # list all nodes + status
bh node info <name>              # which workers are on this node
bh node remove <name>            # decommission a node (migrate workers first)
```

### Worker management (from anywhere, via API to server)

```bash
# Full-time workers
bh worker add <name> \
  --instructions "..."                # server picks a node
bh worker add <name> \
  --instructions "..." \
  --node <node-name>                  # optional: pin to a specific node

# Temp workers
bh worker temp "<task>"          # one-off, server picks a node
bh worker temp "<task>" --node <node-name>  # one-off, specific node

# Remove (own workers only)
bh worker remove <name>

# List
bh worker ls                     # my workers
bh worker ls --all               # all workers in the team
bh worker info <name>            # details: instructions, node, repo, status, created by

# Update
bh worker update <name> \
  --instructions "New instructions"

# Start / stop (own workers only)
bh worker stop <name>
bh worker start <name>

# Logs
bh worker logs <name>
bh worker logs <name> --follow
```

### Delegation (used by lead agents via bash)

```bash
bh delegate <worker> "<task>"    # send task, returns immediately with thread-id
bh wait                          # block until all pending results arrive
```

### Groups & Keys

```bash
bh group create <name>                        # create a group (admin only)
bh group ls                                   # list groups (admin only)
bh group invite <group> --description "..."   # generate group key (admin only)
bh group keys                                 # list API keys
bh group revoke <key-prefix>                  # revoke a key (admin only)
```

## Permissions model

### Group boundary

API key = group boundary. Each group key belongs to a group. Groups are fully isolated from each other.

A single group can have multiple API keys (one per member). Admin keys are server-level and can manage all groups.

### Within a team

| Operation | Who can do it |
|-----------|--------------|
| See all workers | Everyone in the team |
| Send work to any worker | Everyone in the team |
| Add / remove / stop own workers | Creator only |
| Add / remove / stop others' workers | **Not allowed** |

Server tracks `registered_by` (API key) on each agent. Only the registering key can delete or stop it.

### To fix

Server currently does not validate the `from` field on messages. Worker A can send a message with `from: worker-B`. Should enforce `from` = authenticated agent ID.

## What to cut

| Cut | Why |
|-----|-----|
| Topic API | Legacy. Inbox model fully replaces it |
| "Communication layer" as selling point | Nobody cares about the protocol, they care about results |
| 3-terminal getting started | UX disaster |

## What to keep

| Keep | Why |
|------|-----|
| Inbox + thread model | Works well, becomes internal implementation |
| Cross-machine routing | The real differentiator |
| Python SDK | Entry point for non-Claude-Code workers |
| Webhooks | Async notification use cases |

## What to add

| Add | Description |
|-----|-------------|
| Node concept | A machine runs a daemon, accepts scheduling from server |
| Worker daemon | Our own process: polls inbox + invokes LLM |
| `bh server` | Start the control plane (exists, minor changes) |
| `bh node join` | Register a machine as a worker node |
| `bh worker add/remove/ls/temp` | Remote worker lifecycle management |
| `bh delegate` + `bh wait` | Non-blocking task delegation + result collection |
| `bh login` | Connection setup (URL + key) |
| `bh group` | Group and key management |
| Agent skill | `bh skill install <agent>`, teaches agent how to delegate |
| Worker instructions | Per-worker specialization definition |
| Role templates | Preset common worker roles |

## Not now, maybe later

| Feature | When it's needed |
|---------|-----------------|
| Per-group roles (admin / member) | Group exceeds 5-10 people |
| Per-worker access control (who can call whom) | Sensitive workers (e.g., one that can deploy) |
| Audit log | Compliance requirements |
| Worker auto-scaling | Variable load patterns |
| Worker marketplace | Community-shared worker definitions |
