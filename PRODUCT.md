# Box0 - Product Design

**Box0** (`b0`) - an agent platform.

Box0 is a platform for deploying and managing specialized AI agents. It handles node management, worker scheduling, team isolation, and inter-agent communication.

Users don't care how agents communicate. Users care about: I say one thing, a group of specialized agents do their jobs, and results come back.

## Core concepts

| Concept | What it is | Count |
|---------|-----------|-------|
| **Server** | Control plane. Message routing + worker scheduling | 1 |
| **Node** | A machine that can run workers. Runs a daemon, takes orders from server | N |
| **Worker** | An agent process running on a node, with specialized instructions | M |
| **Lead** | The user's own Claude Code session (or any agent with shell access). Not managed by Box0 | - |

- Lead requires no configuration. It's just the user's current Claude Code session.
- Worker specialization comes from the `instructions` field - declarative, each worker has different expertise.
- Server auto-registers itself as a node, so single-machine setups need no extra steps.

## Architecture

```
User's laptop
└── Claude Code (lead)
         │  uses b0 CLI via bash
         ▼
   Box0 Server (control plane)
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
- Instructions defined upfront (single field - name is the short label, instructions is the detail)
- Long-term - stays until explicitly removed
- Anyone on the team can delegate work to it

```bash
b0 worker add reviewer \
  --instructions "Senior code reviewer. Focus on correctness and edge cases. Cite line numbers."

b0 worker add marketer \
  --instructions "Growth marketer. Analyze campaigns and suggest optimizations."
```

Worker definition is **identity only** - who they are, what they're good at. No task-specific details (repo, files, etc.). Those belong in the delegation prompt.

### Temp workers (`worker temp`)

A one-off task. Like hiring a temp/contractor.

- User has a specific task, needs it done now
- Minimal or one-time permissions scoped to the task
- No named role, no persistent definition
- Done when the task is done

```bash
b0 worker temp "look up AWS GPU pricing and summarize options"
b0 worker temp --node gpu-box "process this dataset"
```

## Worker implementation

Workers are **not** Claude Code instances with channels. Headless Claude Code cannot load MCP channels.

A worker is a **daemon process** written by us. It does two things:
1. Polls the Box0 inbox (HTTP long-polling)
2. When a task arrives, invokes an LLM (Claude Code CLI subprocess, or Anthropic API directly) to do the work

Core loop:

```python
while True:
    messages = b0.receive(agent_id, status="unread", timeout=30)
    for msg in messages:
        if msg["type"] == "request":
            result = invoke_llm(msg["content"], instructions=worker_instructions)
            b0.send(to=msg["from"], thread_id=msg["thread_id"],
                        type="done", content=result)
        b0.ack(msg["id"])
```

The platform is runtime-agnostic. The daemon can invoke Claude Code, Codex, a Python script, or any LLM. For MVP, we support Claude Code CLI and direct Anthropic API calls.

Reference implementation: [boxcrew](https://github.com/risingwavelabs/boxcrew) uses a similar pattern - starting Claude Code CLI as a subprocess, capturing NDJSON output.

### Worker context

A worker's capabilities come from two layers:

- **Instructions** → becomes the worker's CLAUDE.md (system prompt). Defines identity and expertise.
- **Node environment** → what's installed on the node (git, shell tools, CLIs, etc.). The toolbox.

All task-specific context (repo URL, branch, diff, data, etc.) comes from the **delegation prompt** written by the lead - not from the worker definition. The same worker can work on different repos, different tasks, different domains. It's the lead's job to provide the right context each time.

When the worker daemon receives a task:
1. Uses the worker's `instructions` as the CLAUDE.md (system prompt)
2. Uses the delegation prompt from the lead as the user message
3. Starts the LLM with both

Node requirements: whatever tools the worker might need should be installed on the node (git, credentials, CLIs, etc.). This is part of node environment setup, not worker configuration.

### Worker auth / LLM credentials

Workers use whatever credentials are already on the node - OAuth first, API key as fallback. Same as the node's environment. No special credential management by Box0, no usage tracking needed initially.

### Worker output

Workers return results through the Box0 inbox message. Output depends on the task type:

- **Analysis/review**: text result in the message content (findings, suggestions, summaries)
- **Code modifications**: worker pushes to a branch, returns branch name/PR URL in the message
- **Data processing**: structured data in the message content (JSON)
- **Non-coding tasks**: text results (campaign analysis, research summaries, etc.)

### The delegation prompt

The quality of the delegation prompt is critical. The lead agent (Claude Code) is responsible for composing a complete, actionable prompt - not just forwarding the user's words.

When the user says "review this PR", the lead must:
1. Understand the user's intent
2. Gather context (which branch, what changed, PR purpose)
3. Compose a prompt the worker can act on - including all task-specific details (repo URL, branch, data, etc.)

```
"Review the changes on branch feature-timeout in repo git@github.com:org/project.git.
This PR adds timeout handling to src/handler.rs.
Check out the branch and focus on correctness and edge cases."
```

Workers are not limited to coding. A marketer worker gets a delegation prompt about campaign data. A researcher worker gets a prompt about a topic to investigate. The instructions (CLAUDE.md) define WHO they are; the delegation prompt defines WHAT to do this time.

**The skill is the product.** The skill installed by `b0 skill install` must teach the agent how to write good delegation prompts - gathering context, including relevant information, composing actionable instructions. This is the core of the lead-side user experience.

## Lead implementation

The lead is any agent with shell access. For MVP, we focus on Claude Code as the lead, but the design is runtime-agnostic - any agent that can run `b0` CLI commands can be a lead.

### MVP: CLI-only (pull-based)

The lead uses `b0` CLI via bash. No MCP, no channel.

```bash
# Send tasks (returns immediately, non-blocking)
b0 delegate reviewer "review this PR"        # → thread-abc
b0 delegate security "check for vulns"        # → thread-def
b0 delegate doc-writer "update the README"    # → thread-ghi

# Wait for results (blocks, streams results as they arrive)
b0 wait
# reviewer done (47s): 2 issues found...
# security done (52s): no vulnerabilities
# doc-writer done (68s): README updated
# All done.
```

**How results arrive**: The lead must explicitly poll via `b0 wait` or `b0 status`. It cannot be notified passively.

**Multi-task concurrency**: User fires off tasks 1, 2, 3 in rapid succession (all non-blocking). Results sit in inbox until the lead checks. The skill instructs Claude Code to run `b0 status` before responding to new user messages, so it can proactively report completed tasks.

**Multi-turn (worker asks a question)**: `b0 wait` returns all pending events in a batch. The lead processes questions, runs `b0 reply`, then calls `b0 wait` again. Or: `b0 orchestrate` command handles the event loop internally, only surfacing questions to the lead when human judgment is needed.

**Pros**: Simple. Universal - any agent with shell access can be a lead. No MCP dependency.
**Cons**: Pull-based. The lead cannot notice background task completions while processing user input. Relies on proactive polling.

### Future: Channel + CLI (push-based)

Not in scope for MVP. Recorded here for future reference.

The lead uses both:
- **box0-channel (MCP)**: receives push notifications (worker results, worker questions)
- **b0 CLI**: sends tasks (`delegate`), manages workers, etc.

```
Lead (Claude Code)
├── box0-channel  → receive: worker results, worker questions (push)
└── b0 CLI      → send: delegate, reply, worker add (pull)
```

**How results arrive**: Box0 channel pushes notifications to the lead. When worker-1 finishes, the lead is immediately notified - even if the user is in the middle of a different conversation. The lead can interleave: "By the way, problem 1 is fixed. Now, about your current question..."

**Multi-task concurrency**: Fully async. User fires off tasks and continues working. Results arrive as push notifications through the channel. No polling needed.

**Multi-turn (worker asks a question)**: Channel pushes the question to the lead. The lead sees it in context, reasons about it, replies via CLI (`b0 reply`). Natural and responsive.

**Pros**: True async. Lead is notified immediately. Better UX for concurrent tasks.
**Cons**: Depends on MCP channel support (experimental). Lead must support MCP - not all agents do. More complex setup.

### How Claude Code knows about Box0

`b0 login` stores connection info. Skill installation is a separate step:

```bash
b0 login http://server:8080 --key <api-key>
# → stores connection info in ~/.b0/config

b0 skill install claude-code   # → ~/.claude/skills/b0/SKILL.md
b0 skill install codex         # → ~/.codex/AGENTS.md

b0 logout
# → clears connection info
# → uninstalls all skills
```

The skill teaches the agent when and how to use `b0` CLI commands. It triggers proactively when the user's request matches delegation patterns (e.g., "review this PR", "check for security issues"). `b0 skill show` prints the content for manual integration with other agents.

If a machine already has `b0 node join` configured (for running workers), `b0 login` is not needed again - the connection info is already stored.

## Worker lifecycle

Workers are ephemeral per-task. The process starts when a request arrives, stays alive for the duration of the task (including multi-turn conversations), and shuts down when the task completes.

```
request arrives → node starts worker process → work + multi-turn → done → process shuts down
```

This is an implementation detail - the user doesn't choose or configure it. From the user's perspective, a worker is a capability that's always available. The platform handles when to start and stop processes.

## User flows

### Single machine (simplest case)

```bash
b0 server              # start server, auto-registers as a node
b0 login http://localhost:8080
b0 worker add reviewer --instructions "Focus on correctness and edge cases."

claude                      # open Claude Code, b0 skill is loaded
```

### Multi-machine

```bash
# Machine A: start server
b0 server

# Machine B: join as a node
b0 node join http://server:8080

# Machine C: join as a node
b0 node join http://server:8080

# From anywhere: manage workers
b0 login http://server:8080
b0 worker add reviewer --instructions "..."              # server picks a node
b0 worker add ml-agent --instructions "..." --node gpu-box  # pin to specific node
```

### Daily use

User opens Claude Code and talks normally:

```
You: review this PR and check for security issues.

Claude Code:
  b0 delegate reviewer "review this PR"
  b0 delegate security "check for vulnerabilities"
  b0 wait

  reviewer done (47s): 2 issues found - unhandled timeout on line 42,
                       generic function name on line 87.
  security done (52s): no vulnerabilities detected.

  Done. Apply the fixes?
```

## CLI design

### Connection

```bash
b0 login http://server:8080 --key <key>  # stores connection info
b0 logout                        # clear credentials + uninstall skill
b0 status                        # which server, connection health
```

### Server (ops, on the server machine)

```bash
b0 server                        # start server
b0 server --config b0.yaml  # start with config file
```

### Node management

```bash
b0 node join http://server:8080  # run on a worker machine, starts daemon
b0 node ls                       # list all nodes + status
b0 node info <name>              # which workers are on this node
b0 node remove <name>            # decommission a node (migrate workers first)
```

### Worker management (from anywhere, via API to server)

```bash
# Full-time workers
b0 worker add <name> \
  --instructions "..."                # server picks a node
b0 worker add <name> \
  --instructions "..." \
  --node <node-name>                  # optional: pin to a specific node

# Temp workers
b0 worker temp "<task>"          # one-off, server picks a node
b0 worker temp "<task>" --node <node-name>  # one-off, specific node

# Remove (own workers only)
b0 worker remove <name>

# List
b0 worker ls                     # my workers
b0 worker ls --all               # all workers in the team
b0 worker info <name>            # details: instructions, node, repo, status, created by

# Update
b0 worker update <name> \
  --instructions "New instructions"

# Start / stop (own workers only)
b0 worker stop <name>
b0 worker start <name>

# Logs
b0 worker logs <name>
b0 worker logs <name> --follow
```

### Delegation (used by lead agents via bash)

```bash
b0 delegate <worker> "<task>"    # send task, returns immediately with thread-id
b0 wait                          # block until all pending results arrive
```

### Groups & Keys

```bash
b0 group create <name>                        # create a group (admin only)
b0 group ls                                   # list your groups
b0 invite <name>                              # create user (admin only)
b0 group add-member <group> <user-id>         # add user to group
```

## Permissions model

### Group boundary

Each user has a unique key. Users belong to groups. Groups are fully isolated from each other.

Groups provide workspace isolation. Workers and messages in one group are invisible to other groups.

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
| `b0 server` | Start the control plane (exists, minor changes) |
| `b0 node join` | Register a machine as a worker node |
| `b0 worker add/remove/ls/temp` | Remote worker lifecycle management |
| `b0 delegate` + `b0 wait` | Non-blocking task delegation + result collection |
| `b0 login` | Connection setup (URL + key) |
| `b0 group` | Group and key management |
| Agent skill | `b0 skill install <agent>`, teaches agent how to delegate |
| Worker instructions | Per-worker specialization definition |
| Role templates | Preset common worker roles |
| Web dashboard | Single-file HTML UI served by the server. Manage workers, tasks, nodes, team from a browser. No separate install. |

## Not now, maybe later

| Feature | When it's needed |
|---------|-----------------|
| Per-group roles (admin / member) | Group exceeds 5-10 people |
| Per-worker access control (who can call whom) | Sensitive workers (e.g., one that can deploy) |
| Audit log | Compliance requirements |
| Worker auto-scaling | Variable load patterns |
| Worker marketplace | Community-shared worker definitions |
