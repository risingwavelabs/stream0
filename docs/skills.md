# Skills

A skill is a [SKILL.md](https://code.claude.com/docs/en/skills.md) file that gets installed into your AI agent. It teaches the agent how to use Box0: when to delegate, what commands to run, and how to write good prompts.

## Install

```bash
b0 skill install claude-code
b0 skill install codex
```

Pick one or both. You only need to do this once per machine.

## What gets installed

**Claude Code**: writes `~/.claude/skills/b0/SKILL.md` with YAML frontmatter and markdown instructions. Claude Code [automatically discovers](https://code.claude.com/docs/en/skills.md) skill files in this directory and loads them when relevant.

The frontmatter includes:

```yaml
---
name: b0
description: |
  Delegate tasks to AI agents via Box0. Use when the user asks to
  review code, check security, run tests, compare tools, get multiple
  perspectives, research a topic, analyze data, write docs, or any
  task that could benefit from specialized or parallel execution.
  Also use when the user mentions agent names or says "ask", "delegate",
  "get opinions from", or "have someone".
allowed-tools:
  - Bash
---
```

Claude uses the `description` field to decide when to auto-load the skill. `allowed-tools: Bash` lets the agent run `b0` commands without permission prompts.

**Codex**: appends a marked section to `~/.codex/AGENTS.md`. Codex reads [AGENTS.md](https://developers.openai.com/codex/guides/agents-md) on startup as custom instructions.

## What the agent learns

The skill body teaches the agent:

- **Discover agents**: run `b0 agent ls`, match agents to the task by description
- **Write detailed prompts**: gather context first (read files, run `git diff`), include specifics, state the deliverable. Never just forward the user's words.
- **Parallel execution**: delegate to multiple agents, then `b0 wait --all`
- **Pipe large content**: `git diff | b0 delegate reviewer "Review this diff."`
- **Handle questions**: answer with `b0 reply`, then `b0 wait` again
- **Multi-turn**: use `--thread` to continue conversations
- **Cron jobs**: schedule recurring tasks with `b0 cron add`
- **Proactive status**: run `b0 status` before responding to check for completed results
- **Error handling**: retry, try a different agent, handle it yourself, or report to user

## View the installed content

```bash
b0 skill show
```

Prints the exact SKILL.md content that gets installed.

## Uninstall

```bash
b0 skill uninstall claude-code
b0 skill uninstall codex
```
