# Box0

Run a team of AI agents, not just one.

Box0 lets you deploy multiple AI agents with different specializations, run them in parallel across one or many machines, and orchestrate them from Claude Code, Codex, or any tool with a shell.

```
You: get three different perspectives on whether Claude Code or Codex
     is better for professional development. then synthesize a conclusion.

Claude Code: I'll delegate to three workers in parallel.

             ux-expert: Claude Code has the better day-to-day experience.
               The 1M context window means you rarely hit limits on large
               codebases, and the skill system makes it extensible...

             architect: Codex's sandbox-first model is architecturally
               cleaner. Every command runs in isolation, which eliminates
               an entire class of accidental side effects...

             pragmatist: In practice, Claude Code ships faster. The tool
               integration is tighter, the agent loop needs less babysitting,
               and the permission model actually works for real teams...

             Based on these three perspectives, here's my synthesis: ...
```

One question from you. Three workers debating in parallel. Your lead agent synthesizes the results. The workers can be on different machines. Box0 routes everything by name.

## Why multi-agent

A single agent is a single perspective, a single thread of execution, and a single machine's resources.

With multiple agents, you can:

- **Get diverse viewpoints.** Different workers with different instructions produce genuinely different analysis.
- **Run tasks in parallel.** A code review, a security audit, and a docs update happen simultaneously instead of sequentially.
- **Distribute across machines.** Put your ML worker on a GPU box, your code reviewer on a dev server, your security auditor on a locked-down machine. Each uses its own local credentials.
- **Separate concerns.** Each worker has focused instructions. A reviewer that only reviews produces better reviews than a generalist doing everything.

## Getting started

This walkthrough uses Claude Code as the lead agent. Box0 itself is runtime-agnostic (see [CLI reference](#cli-reference)), but Claude Code is the easiest way to see it in action.

### 1. Install and start the server

```bash
git clone https://github.com/risingwavelabs/box0.git
cd box0
cargo build --release
export PATH="$PWD/target/release:$PATH"

b0 server
#   Admin key: b0_abc123...
#   Save this key.
```

### 2. Set up a group

```bash
b0 login http://localhost:8080 --key b0_abc123...
b0 group create my-team
b0 group invite my-team --description "me"
#   Key: b0_def456...
b0 login http://localhost:8080 --key b0_def456...
```

### 3. Create workers

```bash
b0 worker add ux-expert \
  --instructions "You are a UX researcher. Evaluate developer tools from the perspective of daily workflow, ergonomics, and productivity."

b0 worker add architect \
  --instructions "You are a software architect. Evaluate tools from the perspective of technical capabilities, extensibility, and system design."

b0 worker add pragmatist \
  --instructions "You are a pragmatic tech lead. Cut through hype. Evaluate based on what actually ships faster with fewer bugs."
```

### 4. Install the skill for your lead agent

For Claude Code:

```bash
b0 skill install claude-code
```

For Codex:

```bash
b0 skill install codex
```

For other agents:

```bash
b0 skill show  # prints skill content to stdout, paste into your agent's instructions
```

### 5. Use it

Open Claude Code (or Codex) and talk normally:

```
You: ask ux-expert, architect, and pragmatist whether Claude Code or Codex
     is better for professional software development. then give me your
     own conclusion based on their arguments.
```

Claude Code will:
1. Run `b0 delegate ux-expert "..."`, `b0 delegate architect "..."`, `b0 delegate pragmatist "..."`
2. Run `b0 wait` to collect all three results
3. Synthesize the arguments and present a conclusion

Three workers, three perspectives, one answer back to you.

## How it works

```
Your agent (lead)          Box0 Server              Worker nodes
     |                         |                        |
     |  b0 delegate reviewer   |                        |
     |  ---------------------->  stores in inbox         |
     |  b0 delegate security   |                        |
     |  ---------------------->  stores in inbox         |
     |                         |                        |
     |                         |   daemon polls inboxes  |
     |                         |   spawns claude CLI     |
     |                         |   <-------- results     |
     |  b0 wait                |                        |
     |  <----------------------  delivers results        |
```

Workers are not long-running processes. When a task arrives, the node daemon spawns `claude --print --output-format json --system-prompt "<instructions>"` as a subprocess. The task is piped via stdin. When done, the result goes back through the inbox to whoever delegated it.

Workers use the machine's existing authentication (OAuth or API key). No special credential setup needed.

## One-off tasks

Don't want to create a named worker? Use `worker temp`:

```bash
b0 worker temp "Summarize the top 5 differences between Rust and Go."
b0 wait
```

Creates a temporary worker, runs the task, auto-cleans up.

## Multi-machine

Run workers on different machines:

```bash
# Machine A: start server
b0 server --host 0.0.0.0

# Machine B: join as a worker node
b0 node join http://machine-a:8080 --name gpu-box --key <key>

# Machine A: add worker on the remote node
b0 worker add ml-agent --instructions "ML specialist." --node gpu-box
b0 delegate ml-agent "Analyze this dataset."
b0 wait
```

The task is routed to Machine B. Claude CLI runs there, using Machine B's credentials and compute.

## CLI reference

```
b0 server [--host] [--port] [--db]       Start server
b0 login <url> --key <key>               Connect
b0 logout                                Disconnect
b0 reset                                 Clean slate
b0 status                                Show connection info

b0 worker add <name> --instructions "..."  [--node <node>]
b0 worker ls / info / update / stop / start / logs / remove
b0 worker temp "<task>"                  One-off task (non-blocking)

b0 delegate <worker> "<task>"            Send task (non-blocking)
b0 delegate <worker>                     Read task from stdin
b0 wait                                  Collect results
b0 reply <thread-id> "<answer>"          Answer a worker's question

b0 node join <url> [--name] [--key]      Join as worker node
b0 node ls                               List nodes

b0 group create / ls / invite / keys / revoke
b0 skill install <agent> / uninstall / show
```

## License

MIT License. Copyright (c) 2026 RisingWave Labs.
