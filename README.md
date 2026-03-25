# Box0: Open-Source Multi-Agent Platform

[![npm](https://img.shields.io/npm/v/@box0/cli)](https://www.npmjs.com/package/@box0/cli)
[![license](https://img.shields.io/github/license/risingwavelabs/box0)](LICENSE)
[![docs](https://img.shields.io/badge/docs-box0-blue)](https://github.com/risingwavelabs/box0/tree/main/docs)
[![SKILL.md](https://img.shields.io/badge/SKILL.md-agent%20onboarding-black)](https://box0.dev/skill.md)

Run AI agents in parallel on one machine or many. Single Rust binary, no dependencies. Works with Claude Code and Codex.

<p align="center">
  <img src="docs/hero.svg" alt="Box0 Architecture" width="800">
</p>

## Agent onboarding

```
Read https://box0.dev/skill.md and follow the instructions to install and configure Box0
```

## Getting started

Install:

```bash
npm install -g @box0/cli@latest
```

Start the server:

```bash
b0 server
```

Teach your agent to use Box0 ([how skills work](docs/skills.md)):

```bash
b0 skill install claude-code
b0 skill install codex
```

Then open Claude Code or Codex and say:

> Create three agents: an optimist, a pessimist, and a realist. Ask them to debate whether AI will replace software engineers in 5 years. Give me your own conclusion.

## Features

**Parallel delegation.** Send tasks to multiple agents at once, collect results when they are done.

```bash
b0 delegate reviewer "Review this PR for correctness."
b0 delegate security "Review this PR for vulnerabilities."
b0 wait --all
```

**Cron jobs.** Schedule recurring tasks.

```bash
b0 cron add --every 6h "Check production logs for errors and summarize."
```

**Webhooks and Slack.** Get notified when agents finish.

```bash
b0 agent add monitor --instructions "Watch for regressions." --webhook https://example.com/hook
b0 agent add alerter --instructions "Triage alerts." --slack "#ops"
```

See [Slack setup](docs/slack.md) for configuration.

**Multi-turn conversations.** Continue where you left off.

```bash
THREAD=$(b0 delegate researcher "Compare Postgres and MySQL for our use case.")
b0 wait
b0 delegate --thread $THREAD researcher "Now factor in DynamoDB."
```

**Pipe content.** Pass files and diffs directly.

```bash
git diff | b0 delegate reviewer "Review this diff."
b0 delegate analyst "Summarize this codebase. @src/"
```

**Temp agents.** One-off tasks, no setup.

```bash
b0 agent temp "List the top 5 differences between Rust and Go."
```

**Multi-machine.** Distribute agents across machines. Each machine uses its own credentials.

```bash
b0 machine join http://server:8080 --name gpu-box --key <key>
b0 agent add ml-agent --instructions "ML specialist." --machine gpu-box
```

**Web dashboard.** Manage agents, view tasks, and monitor machines at `http://localhost:8080`.

## Key concepts

**Server.** The central coordinator. Stores agents, routes tasks, serves the web dashboard. Start with `b0 server`. One server can manage many machines and workspaces.

**Machine.** A computer that runs agents. When you start `b0 server`, the server machine is automatically registered as `local`. Add more machines with `b0 machine join`. Each machine uses its own Claude Code or Codex credentials. Machines belong to the server and are shared across all workspaces.

**Workspace.** A group for organizing agents and controlling team access. Each user gets a personal workspace on signup. Create shared workspaces with `b0 workspace create` and add members with `b0 workspace add-member`.

**Agent.** A named AI worker with a specific role and instructions. Belongs to a workspace, runs on a machine. Created with `b0 agent add`.

## CLI reference

```
b0 server                                    Start server
b0 login <url> --key <key>                   Connect from another machine
b0 status                                    Show connection info
b0 invite <name>                             Create user (admin only)
```

```
b0 agent add <name> --instructions "..."     Create agent
b0 agent ls                                  List agents
b0 agent info <name>                         View agent details
b0 agent logs <name>                         View recent task history
b0 agent stop <name>                         Deactivate agent
b0 agent start <name>                        Reactivate agent
b0 agent remove <name>                       Delete agent
b0 agent temp "<task>"                       One-off task (auto-cleanup)
```

```
b0 delegate <agent> "<task>"                 Send task (non-blocking)
b0 delegate --thread <id> <agent> "<msg>"    Continue conversation
b0 wait [--all] [--timeout <sec>]            Collect results
b0 reply <thread-id> "<answer>"              Answer agent question
b0 threads                                   List recent conversations
```

```
b0 cron add --every <interval> "<task>"      Schedule recurring task
b0 cron ls                                   List scheduled tasks
b0 cron remove <id>                          Delete scheduled task
```

```
b0 machine join <url> --name <id>            Join as remote machine
b0 machine ls                                List machines
```

```
b0 workspace create <name>                   Create workspace
b0 workspace add-member <ws> <user-id>       Add member
b0 skill install claude-code                 Install skill for Claude Code
b0 skill install codex                       Install skill for Codex
```

## Learn more

- [Skills](docs/skills.md) - how skills teach your agent to use Box0
- [Multi-machine setup](docs/multi-machine.md) - distribute agents across machines
- [Cron jobs](docs/cron.md) - schedule recurring tasks
- [Slack notifications](docs/slack.md) - get notified when agents finish
- [Workspaces](docs/teams.md) - share a Box0 server with multiple users
- [Architecture](docs/architecture.md) - task flow, data model, and diagrams
- [CLI reference](docs/cli.md) - full command reference

## License

MIT License. Copyright (c) 2026 RisingWave Labs.
