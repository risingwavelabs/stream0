# Box0 Manual Testing Guide

## Prerequisites

- Claude Code CLI installed and authenticated (`claude --version`)

Install Box0:

```bash
npm install -g @box0/cli@latest
b0 --version
```

Or build from source: `git clone https://github.com/risingwavelabs/box0.git && cd box0 && cargo build --release && export PATH="$PWD/target/release:$PATH"`

---

## Test 1: Server start + first use (no login needed)

```bash
b0 server
```

In another terminal:

```bash
b0 worker add reviewer --description "Code reviewer" --instructions "Answer in one word."
b0 delegate reviewer "Capital of France?"
b0 wait
```

Expected: "reviewer done: Paris". No `--group` needed (default group auto-set).

```bash
b0 worker remove reviewer
```

---

## Test 2: Multi-turn conversation

```bash
b0 worker add debater --description "Debater" --instructions "You are a debater. Max 2 sentences."
```

Round 1:

```bash
b0 delegate debater "Argue that Python is the best language."
b0 wait
```

Round 2 (continue same thread):

```bash
b0 delegate --thread <thread-id-from-round-1> debater "I disagree. Rust is better. Counter my argument."
b0 wait
```

Expected: worker references its previous argument from round 1. If it does not remember round 1, `--resume` is not working.

Round 3:

```bash
b0 delegate --thread <same-thread-id> debater "What about Go?"
b0 wait
```

Expected: worker references all previous rounds.

```bash
b0 worker remove debater
```

---

## Test 3: Worker isolation (separate directories)

```bash
b0 worker add worker-a --instructions "List files in your current directory."
b0 worker add worker-b --instructions "List files in your current directory."
```

```bash
b0 delegate worker-a "Run ls -la in your working directory."
b0 delegate worker-b "Run ls -la in your working directory."
b0 wait
```

Check that `workers/worker-a/` and `workers/worker-b/` exist as separate directories.

```bash
ls workers/
```

---

## Test 4: Invite user + shared group

On the server machine (admin):

```bash
b0 invite alice
b0 group create dev-team
b0 group add-member dev-team <alice-user-id>
b0 group ls
```

As alice (from another terminal or machine):

```bash
b0 login http://localhost:8080 --key <alice-key>
b0 group ls
```

Expected: alice sees her personal group "alice" + "dev-team".

```bash
b0 worker add --group dev-team reviewer --instructions "Be brief."
b0 worker ls --group dev-team
```

Alice cannot see admin's personal workers:

```bash
b0 worker ls --group admin
```

Expected: error (not a member).

---

## Test 5: Worker ownership

```bash
b0 login http://localhost:8080 --key <alice-key>
b0 worker add --group dev-team alice-worker --instructions "x"
```

Admin tries to remove alice's worker:

```bash
b0 login http://localhost:8080 --key <admin-key>
b0 worker remove --group dev-team alice-worker
```

Expected: permission denied.

Alice can remove her own:

```bash
b0 login http://localhost:8080 --key <alice-key>
b0 worker remove --group dev-team alice-worker
```

---

## Test 6: Worker temp + skill install

```bash
b0 worker temp "What is 2+2? Just the number."
b0 wait
```

Expected: prints result, temp worker auto-cleaned.

```bash
b0 skill install claude-code
ls ~/.claude/skills/b0/SKILL.md

b0 skill install codex
head -1 ~/.codex/AGENTS.md

b0 skill uninstall claude-code
b0 skill uninstall codex
```

---

## Test 7: Reset

```bash
b0 reset
ls b0.db 2>&1
```

Expected: DB, config, skills all removed.

---

## Cleanup

```bash
rm -rf ~/.b0/ ~/.claude/skills/b0 b0.db workers/
```
