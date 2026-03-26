# Architecture

## System overview

```
┌─────────────────────────────────────────────────────────────┐
│                        Your Machine                         │
│                                                             │
│   ┌─────────────────┐         ┌──────────────────────────┐ │
│   │   Your Agent    │         │       Box0 Server        │ │
│   │  (Claude Code / │──b0────▶│                          │ │
│   │   Codex / You)  │ delegate│  ┌────────┐  ┌────────┐  │ │
│   └─────────────────┘         │  │ Inbox  │  │  DB    │  │ │
│                               │  └────────┘  └────────┘  │ │
│   ┌─────────────────┐         │       ▲                   │ │
│   │   Web Dashboard │◀────────│       │                   │ │
│   │  (browser :8080)│  serves │       │ poll              │ │
│   └─────────────────┘         └───────┼───────────────────┘ │
│                                       │                     │
│              ┌────────────────────────┼──────────────────┐  │
│              │    Daemon             │                   │  │
│              │                       ▼                   │  │
│              │  ┌──────────┐  ┌──────────┐  ┌──────────┐│  │
│              │  │ agent-1  │  │ agent-2  │  │ agent-3  ││  │
│              │  │(reviewer │  │(security │  │(analyst) ││  │
│              │  │  Claude) │  │  Codex)  │  │  Claude) ││  │
│              │  └──────────┘  └──────────┘  └──────────┘│  │
│              └───────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────┘
```

## Task flow

```
 Your Agent          b0 Server            Daemon               Claude CLI
     │                   │                   │                      │
     │  delegate(task)   │                   │                      │
     │──────────────────▶│                   │                      │
     │                   │  stores in inbox  │                      │
     │  delegate(task2)  │                   │                      │
     │──────────────────▶│                   │                      │
     │                   │                   │                      │
     │                   │◀── poll inbox ────│                      │
     │                   │─── task1 ────────▶│                      │
     │                   │                   │  spawn subprocess    │
     │                   │                   │─────────────────────▶│
     │                   │                   │  pipe task via stdin │
     │                   │                   │                      │ (thinking)
     │                   │                   │◀── result ───────────│
     │                   │◀── write result ──│                      │
     │                   │                   │                      │
     │  b0 wait          │                   │                      │
     │──────────────────▶│                   │                      │
     │◀─── results ──────│                   │                      │
     │                   │                   │                      │
```

## Components

**Server** (`src/server.rs`). Axum HTTP server. Handles API requests, serves the web dashboard, and manages auth middleware. Routes: agents, tasks, machines, users, workspaces, cron, skills.

**Database** (`src/db.rs`). SQLite with WAL mode. Tables: `users`, `workspaces`, `workspace_members`, `agents`, `inbox_messages`, `machines`, `tasks`, `cron_jobs`. Workspace names used as tenants for isolation.

**Daemon** (`src/daemon.rs`). Event-driven processing of agent inboxes. Spawns Claude Code or Codex as subprocesses in each agent's isolated directory. Two modes:
- **Local daemon**: runs inside the server process, direct DB access, woken by inbox notifications.
- **Remote daemon**: runs on remote machines, long-polls server via HTTP.

Max concurrency: 4 concurrent tasks. Timeout: 300 seconds per task (configurable per agent).

**Scheduler** (`src/scheduler.rs`). Runs cron jobs on their configured intervals. Creates inbox messages to trigger agent execution.

**CLI** (`src/main.rs`). Entry point for all subcommands. HTTP client communicates with the server.

**Config** (`src/config.rs`). Server config (host, port, DB path, slack token) and CLI config (server URL, API key, default workspace). Skill installation for Claude Code and Codex.

## Data model

- **Machines** belong to the server, not to workspaces. They are physical compute resources shared across all workspaces. `b0 server` auto-creates a `local` machine. Other machines join via `b0 machine join`. Any workspace's agents can run on any machine.
- **Workspaces** provide tenant isolation for agents and tasks. They do not own machines. Each user gets a personal workspace on creation.
- **Agents** belong to a workspace and are assigned to a machine. Workspace controls who can see the agent. Machine controls where it runs.
- **Users** have unique API keys. Keys identify users, not workspaces.
- **Inbox messages** are the task queue. Each message targets an agent and carries the task content.
- **Tasks** are user-facing work items (Web UI). Each task has a status, conversation thread, and optional sub-tasks.
- **Cron jobs** schedule recurring tasks with configurable intervals and optional end dates.

## Agent execution

1. Task arrives in agent's inbox via `b0 delegate`.
2. Daemon picks up the task and spawns the configured runtime:
   - Claude Code: `claude --print --output-format json --system-prompt "<instructions>"`, task piped via stdin.
   - Codex: `codex exec --json --full-auto --skip-git-repo-check "<instructions>\n\n<task>"`.
3. Runtime output is parsed and stored as the response.
4. For multi-turn conversations, the Claude session ID is stored and used with `--resume` on follow-up messages. Codex does not support session resume.
5. On completion, webhooks are fired and Slack notifications sent if configured.

## Auth model

- Users authenticate via API key in the `X-API-Key` header.
- Each user can be in multiple workspaces. `--workspace` selects the operating context.
- Agents track `registered_by`. Only the creator can remove, update, or stop their agents.
- Machines are owned by users. Only the owner can deploy agents to their machine.
- Admin user is created on first server start.
