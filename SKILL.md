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

# Box0 (`b0`) Multi-Agent Platform

Run AI agents in parallel on one machine or many. Delegate tasks, collect results, schedule cron jobs.

## Setup

### Step 1: Check if Box0 is installed

```bash
b0 --version
```

If this succeeds, skip to Step 3.

### Step 2: Install

```bash
npm install -g @box0/cli@latest
```

If npm is not available, build from source:

```bash
git clone https://github.com/risingwavelabs/box0.git
cd box0 && cargo build --release
export PATH="$PWD/target/release:$PATH"
```

### Step 3: Check if server is running

```bash
b0 status
```

If this shows "Status: connected", skip to Step 5.

### Step 4: Connect to a server

**Option A: Start a local server (self-hosted)**

Run in a separate terminal or background process:

```bash
b0 server
```

On first start, Box0 creates an admin account and auto-configures `~/.b0/config.toml`.

**Option B: Connect to a remote server (cloud/team)**

If the user already has a remote Box0 server, log in instead of starting a local one:

```bash
b0 login <server-url> --key <api-key>
```

The user must provide the server URL and API key. After login, continue to Step 5.

### Step 5: Register this machine (remote servers only)

If `b0 status` shows a remote server (not `localhost` or `127.0.0.1`), check if this machine is registered:

```bash
b0 machine ls
```

If no machines are listed, or if agent creation later fails with "no local machine", register this machine. Read `server_url` and `api_key` from `~/.b0/config.toml`, then run:

```bash
b0 machine join <server-url> --name <hostname> --key <api-key>
```

Replace `<server-url>` with the server URL, `<hostname>` with this machine's hostname, and `<api-key>` with the API key from the config file.

If the server is local (localhost or 127.0.0.1), skip this step - the local machine is registered automatically.

### Step 6: Install the skill

```bash
which claude && b0 skill install claude-code
which codex && b0 skill install codex
```

On Windows, use `where` instead of `which`.

### Step 7: Verify

```bash
b0 agent ls
```

This should run without errors. Setup is complete.

Tell the user: "Box0 is installed and ready. You can now delegate tasks to agents."

---

## When to use

When the user's request could benefit from specialized agents or parallel execution, delegate.

## Choosing an agent

**Always use temp agents unless the user explicitly names an existing agent.** `b0 agent temp "<task>"` is the default for everything. No setup, no cleanup, no `b0 agent add`. Even if the user says "find 3 agents" or "use multiple agents", create 3 temp agents with `b0 agent temp`.

**Only use `b0 agent add` when:**
- The user explicitly says "create a permanent agent" or "add an agent that I can reuse"
- Never for one-off tasks, debates, research, reviews, or any task that will be done today

**Only use `b0 delegate <name>` when:**
- `b0 agent ls` shows an existing agent that matches the task
- The user mentions an agent by name ("ask the reviewer")

## Commands

```bash
b0 agent ls                                           # list available agents
b0 delegate <agent> "<detailed task prompt>"          # send task (non-blocking)
b0 delegate --thread <id> <agent> "<follow-up>"       # continue conversation
b0 wait                                                # wait for next completed result
b0 wait --all                                          # wait for all pending results
b0 wait --timeout 0                                    # non-blocking check for completed results
b0 reply <thread-id> "<answer>"                        # answer an agent's question
b0 status                                              # check pending tasks
b0 agent temp "<task>"                                 # one-off task, no named agent
b0 agent add <name> --instructions "..."               # create a named agent
b0 agent remove <name>                                 # delete an agent
b0 cron add --every <interval> "<task>"                # schedule recurring task (auto-creates temp agent)
b0 cron add --agent <name> --every <interval> "<task>" # schedule with existing agent
b0 cron ls                                             # list scheduled tasks
b0 cron remove <id>                                    # remove a scheduled task
```

## How to write delegation prompts

This is critical. Do NOT forward the user's words. Compose a complete, actionable prompt.

Bad:
```
b0 delegate reviewer "review this PR"
```

Good:
```
b0 delegate reviewer "Review the changes on branch feature-timeout in this repo.
The PR adds timeout handling to src/handler.rs.
Focus on correctness, edge cases, and error handling.
Cite line numbers for any issues found."
```

Steps:
1. **Gather context first** - read relevant files, run `git diff`, check the branch
2. **Include specifics** - file paths, line numbers, branch names, what changed and why
3. **State the deliverable** - what the agent should produce (a list of issues, a summary, a fix)

For large content (diffs, file contents), pipe via stdin:
```
git diff main..HEAD | b0 delegate reviewer "Review the following diff. Focus on correctness."
```

## Concurrent tasks

Delegate to multiple agents, then collect all results:

```bash
b0 delegate reviewer "Review the changes on branch feature-timeout..."
b0 delegate security "Check src/handler.rs for OWASP top 10 vulnerabilities..."
b0 delegate doc-writer "Update README to reflect the new timeout config option..."
b0 wait --all
```

All three run in parallel. `b0 wait --all` blocks until all complete.

## Handling agent questions

During `b0 wait`, an agent may ask a question:

```
reviewer asks (thread thread-abc): "Is the timeout change on line 42 intentional?"
  -> Use: b0 reply thread-abc "<your answer>"
```

Answer with `b0 reply`, then run `b0 wait` again to continue collecting results.

## Proactive status checks

Before responding to a new user message, run `b0 status` to check if any previously delegated tasks have completed. Report results to the user if any are ready.

## Error handling

If an agent fails, `b0 wait` reports it. Decide whether to:
- Retry with a clearer prompt
- Try a different agent
- Handle the task yourself
- Report the failure to the user

## Multi-turn conversations

To continue a conversation with an agent, pass the thread ID from the first round:

```bash
b0 delegate --thread <thread-id> <agent> "<follow-up>"
b0 wait
```

The agent remembers all previous turns.

## Troubleshooting

| Symptom | Fix |
|---------|-----|
| `b0: command not found` | Run `npm install -g @box0/cli@latest` |
| `b0 status` shows no connection | Start the server with `b0 server` |
| `b0 delegate` hangs | Check that the daemon is running (it starts with the server) |
| Agent returns empty result | Check agent instructions with `b0 agent info <name>` |
| "no local machine" error | Run `b0 machine join <url> --name <hostname> --key <key>` to register this machine |
| Timeout errors | Default is 300s. Check if the task needs more time. |
