# Box0

An open-source agent swarm platform. Run multiple AI agents in parallel on your laptop or across a fleet of machines. Define agents with different roles, delegate tasks, and collect results. Single Rust binary with built-in web dashboard. Works with Claude Code and Codex.

## Why Box0

More agents means more work done. But running a swarm of agents raises real problems: where do they run, how do you assign work, how do you add machines, and how do you know what each agent is doing.

Box0 handles all of this:

- **Run anywhere.** Start on your laptop with a single command. Add remote machines as nodes when you need more compute. Each node uses its own credentials.
- **Orchestrate.** Your agent delegates tasks to workers in parallel. Box0 queues, dispatches, and routes each task to the right node. Results flow back automatically.
- **Manage.** Add and remove workers, assign them to machines, organize teams with access control. One binary, no external dependencies.
- **Observe.** Built-in web dashboard shows every worker, task, and result in real time. Logs are always accessible.

## Use cases

**Multi-perspective debate.** Three agents with different viewpoints argue the same question. You get a synthesized conclusion.

> Create an optimist, a pessimist, and a realist. Ask them to debate whether we should rewrite our backend in Rust. Summarize their arguments.

**Parallel code review.** Three reviewers examine the same diff simultaneously: correctness, security, performance. Results come back together.

> Send this diff to the correctness-reviewer, security-reviewer, and perf-reviewer. Compile their feedback into one report.

**Fan-out research.** One agent per topic. All research in parallel, then your agent compares findings.

> Create 5 workers. Each one evaluates a different database: Postgres, MySQL, SQLite, DuckDB, RisingWave. Compare their findings.

**Divide and conquer.** Split a large task by module or file. Each agent handles one piece.

> Split the migration of src/api/ into three parts by subdirectory. Assign one worker to each. Migrate them to the new auth pattern.

**Red team / blue team.** One agent builds, another attacks. Adversarial review as a workflow.

> Have the implementer add input validation to the signup form. Then have the attacker try to bypass it. Iterate until the attacker gives up.

## How it works (single-node scenario)

```
┌─────────────────────────────────────────────────────────────┐
│                        Your Machine                         │
│                                                             │
│   ┌─────────────────┐         ┌───────────────────────────┐ │
│   │   Your Agent    │         │       Box0 Server         │ │
│   │  (Claude Code / │──b0────▶│                           │ │
│   │   Codex / You)  │ delegate│  ┌────────┐  ┌────────┐   │ │
│   └─────────────────┘         │  │ Inbox  │  │  DB    │   │ │
│                               │  └────────┘  └────────┘   │ │
│   ┌─────────────────┐         │        ▲                  │ │
│   │   Web Dashboard │◀────────│        │                  │ │
│   │  (browser :8080)│  serves │        │ poll             │ │
│   └─────────────────┘         └────────┼──────────────────┘ │
│                                        │                    │
│              ┌──────────────────────── ┼──────────────────┐ │
│              │    Node Daemon          │                  │ │
│              │                         ▼                  │ │
│              │  ┌──────────┐  ┌──────────┐  ┌──────────┐  │ │
│              │  │ worker-1 │  │ worker-2 │  │ worker-3 │  │ │
│              │  │(optimist │  │(pessimist│  │(realist) │  │ │
│              │  │  Claude) │  │  Codex)  │  │  Claude) │  │ │
│              │  └──────────┘  └──────────┘  └──────────┘  │ │
│              └────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

Your agent sends tasks to the Box0 server via `b0 delegate`. The server stores them in an inbox. A node daemon polls the inbox, spawns a separate Claude Code (or Codex) process for each worker, and writes the results back. Your agent calls `b0 wait` to collect the responses.

Each worker runs in its own isolated directory. Workers can also run across multiple machines. See [Multi-machine](docs/multi-machine.md).

## Getting started

### 1. Install

```bash
npm install -g @box0/cli@latest
```

Or build from source:

```bash
git clone https://github.com/risingwavelabs/box0.git
cd box0 && cargo build --release
export PATH="$PWD/target/release:$PATH"
```

### 2. Start the server

```bash
b0 server
```

On first start, Box0 creates an admin account and prints your API key.

### 3. Teach your agent to use Box0

For Claude Code:

```bash
b0 skill install claude-code
```

For Codex:

```bash
b0 skill install codex
```

You only need to do this once.

### 4. Try it

Open Claude Code and say:

> Create three workers: an optimist, a pessimist, and a realist. Ask them to debate whether AI will replace software engineers in 5 years. Then give me your own conclusion.

Your agent creates the workers, runs the debate in parallel, and synthesizes the results.

### What just happened

When you asked your agent to run the debate, here is what happened behind the scenes:

1. Your agent called `b0 worker add` to create three workers, each with different instructions.
2. Your agent called `b0 delegate` to send the debate topic to all three workers in parallel.
3. Box0 dispatched each task to a separate Claude Code process, running simultaneously in isolated directories.
4. Your agent called `b0 wait` to collect the three responses.
5. Your agent read the responses and synthesized a final answer for you.

You only typed one message. Your agent handled the rest through Box0's CLI.

## What you can do

**Add a worker:**

```bash
b0 worker add reviewer --instructions "You are a senior code reviewer." --description "Code reviewer"
```

**List workers:**

```bash
b0 worker ls
```

**View worker logs:**

```bash
b0 worker logs <name>
```

**Remove a worker:**

```bash
b0 worker remove <name>
```

**Run a one-off task** (temporary worker, auto-cleans up):

```bash
b0 worker temp "Summarize the top 5 differences between Rust and Go."
```

These are the commands you run. Everything else is handled by your agent automatically.

## Concepts

| Concept | What it is |
|---------|-----------|
| **Worker** | A named agent with a specific role. Runs in its own isolated directory. |
| **Group** | A workspace for sharing workers among team members. |
| **Node** | A machine that runs workers. The server is always a node. Others join via `b0 node join`. |
| **Skill** | Instructions installed into your agent (Claude/Codex) that teach it how to use Box0. |

## Learn more

- [Multi-machine setup](docs/multi-machine.md) — distribute workers across machines
- [Teams](docs/teams.md) — share a Box0 server with multiple users
- [Architecture](docs/architecture.md) — task flow, data model, and detailed diagrams
- [CLI reference](docs/cli.md) — full command reference including agent-facing commands

## Web dashboard

Open your browser to the server URL (default `http://localhost:8080`) and log in with your API key. Manage workers, view tasks, monitor nodes, and manage your team from the UI.

## License

MIT License. Copyright (c) 2026 RisingWave Labs.
