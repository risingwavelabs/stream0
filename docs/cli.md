# CLI reference

## Server

```
b0 server [--host] [--port] [--db]         Start server
```

## Authentication

```
b0 login <url> --key <key>                 Connect from another machine
b0 logout                                  Disconnect
b0 reset                                   Clean slate (deletes DB, config, skills)
b0 status                                  Show connection info and pending tasks
b0 invite <name>                           Create user (admin only)
```

## Agents

```
b0 agent add <name> --instructions "..." [--description "..."] [--workspace <w>] [--machine <m>] [--runtime auto|claude|codex] [--webhook <url>] [--slack <channel>]
b0 agent ls [--workspace <w>]
b0 agent info <name> [--workspace <w>]
b0 agent update <name> --instructions "..." [--workspace <w>]
b0 agent stop <name> [--workspace <w>]
b0 agent start <name> [--workspace <w>]
b0 agent logs <name> [--workspace <w>]
b0 agent remove <name> [--workspace <w>]
b0 agent temp "<task>" [--workspace <w>]   One-off task (non-blocking, auto-cleanup)
```

## Task delegation

These commands are primarily used by agents, not humans.

```
b0 delegate <agent> "<task>" [--workspace <w>]       New task (non-blocking)
b0 delegate --thread <id> <agent> "<message>"        Continue conversation
b0 delegate <agent>                                  Read task from stdin
b0 wait [--all] [--timeout <sec>]                    Collect results
b0 reply [--workspace <w>] <thread-id> "<answer>"    Answer an agent's question
b0 threads [--workspace <w>] [--limit <n>]           List recent conversations
```

### How delegation works

1. `b0 delegate` sends a task to an agent's inbox and returns immediately with a thread ID.
2. The daemon picks up the task, spawns a Claude Code or Codex process, and executes it.
3. `b0 wait` blocks until a pending task has results, then prints them. Use `--all` to wait for everything.
4. For multi-turn conversations, pass `--thread <id>` to continue an existing conversation. The agent resumes its Claude session with full history.

## Cron jobs

```
b0 cron add --every <interval> "<task>" [--agent <name>] [--workspace <w>] [--webhook <url>] [--slack <channel>] [--until <date>]
b0 cron ls [--workspace <w>]
b0 cron remove <id> [--workspace <w>]
b0 cron enable <id> [--workspace <w>]
b0 cron disable <id> [--workspace <w>]
```

Intervals: `30s`, `5m`, `1h`, `6h`, `1d`. Optional end date: `2026-04-24` or `2026-04-24T12:00:00Z`.

If `--agent` is omitted, a temporary agent is auto-created and cleaned up when the cron job is removed.

## Machines

```
b0 machine join <url> [--name <id>] [--key <key>]    Join as remote machine
b0 machine ls                                         List machines
```

## Workspaces

```
b0 workspace create <name>                 Create workspace
b0 workspace ls                            List your workspaces
b0 workspace add-member <workspace> <user-id>   Add user to workspace
```

## Skills

```
b0 skill install claude-code               Install Box0 skill for Claude Code
b0 skill install codex                     Install Box0 skill for Codex
b0 skill uninstall <agent>                 Remove installed skill
b0 skill show                              Print skill content to stdout
```

### What skills do

Skills teach your agent how to use Box0. When installed:

- **Claude Code**: writes a skill file to `~/.claude/skills/b0/SKILL.md`. Claude Code reads this and learns the `b0 delegate` / `b0 wait` workflow.
- **Codex**: appends a marked section to `~/.codex/AGENTS.md`.

After installation, your agent knows how to create agents, delegate tasks, and collect results without any manual instruction.
