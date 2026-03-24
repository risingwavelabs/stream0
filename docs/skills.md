# Skills

A skill is a set of instructions that gets installed into your AI agent (Claude Code or Codex). Once installed, the agent reads these instructions and learns how to use Box0 autonomously. You don't need to teach it anything manually.

## Install

```bash
b0 skill install claude-code
b0 skill install codex
```

Pick one or both, depending on which agent you use. You only need to do this once per machine.

## Where it gets installed

- **Claude Code**: `~/.claude/skills/b0/SKILL.md`. Claude Code automatically loads skill files from this directory.
- **Codex**: appends a marked section to `~/.codex/AGENTS.md`. Codex reads this file on startup.

## What the agent learns

The skill teaches your agent to:

1. **Discover available agents** by running `b0 agent ls` and matching them to the task by description.
2. **Compose detailed delegation prompts** with full context (file paths, diffs, branch names), not just forwarding the user's words.
3. **Delegate tasks in parallel** to multiple agents and collect results with `b0 wait`.
4. **Pipe large content** (diffs, file contents) via stdin.
5. **Handle agent questions** by answering with `b0 reply` and resuming `b0 wait`.
6. **Continue multi-turn conversations** using `--thread`.
7. **Schedule recurring tasks** with `b0 cron add`.
8. **Proactively check status** before responding to new user messages, so completed results are reported automatically.

The skill also includes prompt engineering guidance: examples of bad vs. good delegation prompts, and error handling strategies (retry, fallback, or escalate).

## View the full skill content

```bash
b0 skill show
```

This prints the exact instructions that your agent receives.

## Uninstall

```bash
b0 skill uninstall claude-code
b0 skill uninstall codex
```

## How it connects

After installation, the typical flow is:

1. You ask your agent something (e.g., "Review this PR from three angles").
2. The agent reads the skill, recognizes it should delegate, runs `b0 agent ls`.
3. The agent composes detailed prompts and runs `b0 delegate` for each sub-task.
4. Box0 dispatches tasks to the appropriate agents in parallel.
5. The agent runs `b0 wait`, collects results, and synthesizes a response for you.

You only type one message. The skill handles the rest.
