# Box0

Box0 is a multi-agent platform. It lets you run multiple AI agents with different specializations across one or more machines, assign tasks to them in parallel, and collect results.

## Problem

AI coding agents like Claude Code and Codex work alone. One agent handles everything sequentially: code review, security audit, documentation, testing. If you want multiple perspectives on the same question, or need several tasks done at once, you wait for one agent to finish before it can start the next.

There is no standard way to run multiple agents as a team, split work between them, or distribute them across machines.

## Solution

Box0 provides the infrastructure to:

- **Run multiple agents in parallel.** Define agents with different instructions. They execute concurrently, each in its own isolated workspace.
- **Multi-turn conversations.** Continue a conversation with a worker across multiple rounds using `--thread`. The worker remembers all previous turns.
- **Distribute across machines.** Agents can run on your laptop, a GPU server, or any machine. Box0 routes tasks by name. Each machine uses its own local credentials.
- **Integrate with existing tools.** Claude Code and Codex can learn to use Box0 automatically through skill installation. No workflow changes required.
- **Isolate teams.** Groups provide workspace isolation. Multiple users or teams share one Box0 server without seeing each other's agents or data.

## Install

```bash
npm install -g @box0/cli@latest
```

Or run directly without installing:

```bash
npx @box0/cli@latest server
```

Or build from source:

```bash
git clone https://github.com/risingwavelabs/box0.git
cd box0 && cargo build --release
export PATH="$PWD/target/release:$PATH"
```

## Getting started

This walkthrough uses Claude Code. Box0 also works with Codex or any tool that can run shell commands.

### 1. Start the server

In a separate terminal:

```bash
b0 server
```

On first start, Box0 creates an admin user with a personal group called "admin" and prints the admin key. The CLI is auto-configured on the server machine, no login needed.

### 2. Create workers

```bash
b0 worker add ux-expert --description "UX researcher" \
  --instructions "You are a UX researcher. Evaluate developer tools from the perspective of daily workflow, ergonomics, and productivity."

b0 worker add architect --description "Software architect" \
  --instructions "You are a software architect. Evaluate tools from the perspective of technical capabilities, extensibility, and system design."

b0 worker add pragmatist --description "Pragmatic tech lead" \
  --instructions "You are a pragmatic tech lead. Cut through hype. Evaluate based on what actually ships faster with fewer bugs."
```

When `--group` is omitted, the default group (set during server setup) is used.

### 3. Install the skill for Claude Code (or Codex)

For Claude Code:

```bash
b0 skill install claude-code
```

For Codex:

```bash
b0 skill install codex
```

For other agents, run `b0 skill show` to print the skill content. Paste it into your agent's custom instructions.

### 4. Use it

Open Claude Code (or Codex) and say something like:

> ask ux-expert, architect, and pragmatist whether Claude Code or Codex is better for professional software development. then give me your own conclusion based on their arguments.

Claude Code automatically calls `b0 delegate` for each worker, runs `b0 wait` to collect the results, and synthesizes a conclusion. Three agents, three perspectives, one answer.

## Multi-turn conversations

Continue a conversation with a worker by passing `--thread`:

```bash
b0 delegate debater "Argue that Python is the best language."
```

This prints a thread ID (e.g. `thread-abc123`). After collecting the result with `b0 wait`, continue the conversation:

```bash
b0 delegate --thread thread-abc123 debater "I disagree. Rust is better for safety. Counter my argument."
b0 wait
```

The worker remembers everything from previous turns. Each follow-up builds on the full conversation history.

## Adding team members

On the server machine (admin):

```bash
b0 invite alice
b0 group create dev-team
b0 group add-member dev-team <alice-user-id>
```

On Alice's laptop:

```bash
b0 login http://server:8080 --key <alice-key>
b0 worker add --group dev-team reviewer --instructions "Code reviewer."
b0 delegate --group dev-team reviewer "Review src/main.rs"
b0 wait
```

Each user has their own key. Users can be in multiple groups. Workers in a group are visible to all group members.

## How it works

```
Your agent               Box0 Server              Worker nodes
     |                         |                        |
     |  b0 delegate reviewer   |                        |
     |  ---------------------->  stores in inbox         |
     |  b0 delegate security   |                        |
     |  ---------------------->  stores in inbox         |
     |                         |                        |
     |                         |   daemon polls inboxes  |
     |                         |   spawns claude CLI     |
     |                         |   in worker's own dir   |
     |                         |   <-------- results     |
     |  b0 wait                |                        |
     |  <----------------------  delivers results        |
```

Each worker runs in its own isolated directory under `workers/<name>/`. When a task arrives, the node daemon spawns `claude --print --output-format json --system-prompt "<instructions>"` as a subprocess in that directory. The task is piped via stdin. When done, the result goes back through the inbox.

For multi-turn conversations, the daemon stores the Claude session ID and uses `--resume` on follow-up messages, preserving the full conversation history.

Workers use the machine's existing authentication (OAuth or API key). No special credential setup needed.

## One-off tasks

```bash
b0 worker temp "Summarize the top 5 differences between Rust and Go."
b0 wait
```

Creates a temporary worker, runs the task, auto-cleans up.

## Multi-machine

```bash
# Machine A: start server
b0 server

# Machine B: join as a worker node
b0 node join http://machine-a:8080 --name gpu-box --key <key>

# Machine A: add worker on the remote node
b0 worker add ml-agent --instructions "ML specialist." --node gpu-box
b0 delegate ml-agent "Analyze this dataset."
b0 wait
```

The task is routed to Machine B. Claude CLI runs there, using Machine B's credentials and compute. Only the node owner can deploy workers to their machine.

## CLI reference

```
b0 server [--host] [--port] [--db]         Start server
b0 login <url> --key <key>                 Connect from another machine
b0 logout                                  Disconnect
b0 reset                                   Clean slate
b0 status                                  Show connection info
b0 invite <name>                           Create user (admin only)

b0 worker add <name> --instructions "..." [--description "..."] [--group <g>] [--node <n>]
b0 worker ls [--group <g>]
b0 worker info / update / stop / start / logs / remove [--group <g>] <name>
b0 worker temp "<task>" [--group <g>]      One-off task (non-blocking)

b0 delegate <worker> "<task>" [--group <g>]       New task (non-blocking)
b0 delegate --thread <id> <worker> "<message>"    Continue conversation
b0 delegate <worker>                              Read task from stdin
b0 wait                                           Collect results
b0 reply [--group <g>] <thread-id> "<answer>"     Answer a worker question

b0 node join <url> [--name] [--key]        Join as worker node
b0 node ls                                 List nodes

b0 group create <name>                     Create group
b0 group ls                                List your groups
b0 group add-member <group> <user-id>      Add user to group

b0 skill install claude-code / codex       Install agent skill
b0 skill uninstall <agent>                 Remove
b0 skill show                              Print to stdout
```

## License

MIT License. Copyright (c) 2026 RisingWave Labs.
