# Boxhouse Manual Testing Guide

## Prerequisites

- Rust toolchain installed
- Claude Code CLI installed and authenticated (run `claude --version` to verify)
- For multi-machine tests: two machines that can reach each other over the network

Build first:

```bash
cd boxhouse
cargo build --release
export PATH="$PWD/target/release:$PATH"
```

Verify:

```bash
bh --version
bh --help
```

---

## Test 1: Server Bootstrap + Login

**Goal**: Verify server starts, generates admin key, and login works.

Terminal 1 — start server:
```bash
bh server
# Expected:
#   Admin key: bh_<long-key>
#   Save this key. Use it to login:
#   bh login http://127.0.0.1:8080 --key bh_<long-key>
```

Terminal 2 — login:
```bash
# Login with the admin key printed above
bh login http://localhost:8080 --key <admin-key>
# Expected: "Connected", "Login complete."
# Expected: "To install agent skill: bh skill install claude-code  (or: codex)"

# Install skill (separate step)
bh skill install claude-code
# Expected: "Skill installed for Claude Code (~/.claude/skills/bh/SKILL.md)"

ls ~/.claude/skills/bh/SKILL.md
cat ~/.claude/skills/bh/SKILL.md | head -5
# Expected: YAML frontmatter with name: bh

# Or for Codex:
bh skill install codex
# Expected: "Skill installed for Codex (~/.codex/AGENTS.md)"

# Verify config
cat ~/.bh/config.toml
# Expected: server_url, lead_id, api_key
```

### What to Check

- [ ] Server prints admin key on first start
- [ ] Second start does NOT print a new key (reuses existing)
- [ ] `bh login --key` succeeds
- [ ] Skill file created at `~/.claude/skills/bh/SKILL.md`
- [ ] Config saved at `~/.bh/config.toml` with api_key

---

## Test 2: Groups + Access Control

**Goal**: Verify group isolation and admin-only operations.

```bash
# As admin:
bh group create frontend
bh group create ml-team
bh group ls
# Expected: 2 groups

# Invite members
bh group invite frontend --description "alice"
# Expected: prints key for alice. SAVE IT.

bh group invite ml-team --description "bob"
# Expected: prints key for bob. SAVE IT.

# List all keys (admin sees all)
bh group keys
# Expected: 3 keys (admin + alice + bob) with role and group columns

# Login as alice
bh login http://localhost:8080 --key <alice-key>
bh worker add reviewer --instructions "Review code."
bh worker ls
# Expected: only sees reviewer

# Login as bob
bh login http://localhost:8080 --key <bob-key>
bh worker add ml-agent --instructions "ML tasks."
bh worker ls
# Expected: only sees ml-agent (NOT reviewer)

# Non-admin cannot create groups
bh group create hacked
# Expected: "Error: admin key required"
```

### What to Check

- [ ] Groups created successfully
- [ ] Group keys scoped to their group
- [ ] Alice only sees frontend workers
- [ ] Bob only sees ml-team workers
- [ ] Non-admin cannot create groups or revoke keys

---

## Test 3: Basic Worker Flow

**Goal**: Verify delegate + wait end-to-end.

```bash
bh login http://localhost:8080 --key <group-key>
bh worker add reviewer --instructions "Be concise — max 1 sentence."
bh delegate reviewer "Is Rust a good language?"
# Expected: prints thread-id immediately (non-blocking)

bh wait
# Expected: blocks, then prints result

bh worker remove reviewer
```

---

## Test 4: Worker Temp

**Goal**: Verify one-off tasks work (non-blocking).

```bash
bh worker temp "What is 2+2? Just the number."
# Expected: prints thread-id immediately

bh wait
# Expected: prints result, temp worker auto-cleaned

bh worker ls
# Expected: no workers (temp worker removed)
```

---

## Test 5: Delegate from Stdin

**Goal**: Verify large content can be piped via stdin.

```bash
echo "List the first 5 prime numbers." | bh delegate reviewer
# Expected: prints thread-id

bh wait
# Expected: prints result
```

---

## Test 6: Worker Lifecycle

**Goal**: Verify info, update, stop, start, logs.

```bash
bh worker add test-worker --instructions "Be brief."
bh worker info test-worker
# Expected: shows name, node, status, registered_by, instructions

bh worker update test-worker --instructions "Be very brief."
bh worker info test-worker | grep Instructions
# Expected: "Be very brief."

bh worker stop test-worker
bh worker ls
# Expected: status = stopped

bh worker start test-worker
bh worker ls
# Expected: status = active

# Delegate and check logs
bh delegate test-worker "Say hello"
bh wait
bh worker logs test-worker
# Expected: shows request + done messages

bh worker remove test-worker
```

---

## Test 7: Worker Ownership

**Goal**: Verify only creator can modify/delete workers.

Requires two different group keys (alice and bob in the same group, or admin + member).

```bash
# Login as alice, create a worker
bh login http://localhost:8080 --key <alice-key>
bh worker add alice-worker --instructions "x"

# Login as bob, try to remove it
bh login http://localhost:8080 --key <bob-key>
bh worker remove alice-worker
# Expected: "Error: permission denied: worker was created by someone else"

bh worker stop alice-worker
# Expected: "Error: permission denied"

# Bob CAN see and delegate to it
bh worker ls
# Expected: alice-worker is listed

# Alice can remove her own worker
bh login http://localhost:8080 --key <alice-key>
bh worker remove alice-worker
# Expected: success
```

---

## Test 8: Multi-Machine

**Goal**: Verify remote nodes.

Machine A — start server:
```bash
bh server --host 0.0.0.0 --port 8080
# Save the admin key

bh login http://localhost:8080 --key <admin-key>
bh group create team
bh group invite team --description "node-key"
# Save the group key
```

Machine B — join as node:
```bash
bh node join http://<machine-a-ip>:8080 --name remote-box --key <group-key>
# Expected: "Joining as node" + daemon starts
```

Machine A — use remote node:
```bash
bh login http://localhost:8080 --key <group-key>
bh node ls
# Expected: local + remote-box

bh worker add remote-w --instructions "Be brief." --node remote-box
bh delegate remote-w "What is 1+1?"
bh wait
# Expected: result comes back from remote node
```

### Troubleshooting

- Machine A must bind to `0.0.0.0` (not `127.0.0.1`)
- Check firewall on port 8080
- Verify with `curl http://<ip>:8080/health` from Machine B
- Claude Code CLI must be installed on Machine B

---

## Test 9: Edge Cases

```bash
# Delegate to nonexistent worker
bh delegate nonexistent "hello"
# Expected: error message

# Wait with no pending tasks
bh wait
# Expected: "No pending tasks."

# Login without key
bh login http://localhost:8080
# Expected: works (health check is public), but subsequent commands fail

# Server not running
bh worker ls
# Expected: connection error
```

---

## Cleanup

```bash
# Stop server (Ctrl+C)
# Stop remote daemons (Ctrl+C)

rm -rf ~/.bh/
rm -rf ~/.claude/skills/bh
rm -f bh.db
```
