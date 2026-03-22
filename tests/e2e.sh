#!/usr/bin/env bash
#
# Box0 end-to-end test. Requires Claude Code or Codex installed.
# Starts a real server, runs CLI commands, and verifies results.
#
# Usage: ./tests/e2e.sh
#
set -euo pipefail

PORT=9876
DB_PATH="/tmp/b0-e2e-test.db"
B0="b0"
PASS=0
FAIL=0

cleanup() {
    if [ -n "${SERVER_PID:-}" ]; then
        kill "$SERVER_PID" 2>/dev/null || true
        wait "$SERVER_PID" 2>/dev/null || true
    fi
    rm -f "$DB_PATH" "${DB_PATH}-wal" "${DB_PATH}-shm"
    rm -rf /tmp/b0-e2e-workers
}
trap cleanup EXIT

log() { echo "=== $1 ==="; }
pass() { echo "  PASS: $1"; PASS=$((PASS + 1)); }
fail() { echo "  FAIL: $1"; FAIL=$((FAIL + 1)); }

check() {
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        pass "$desc"
    else
        fail "$desc"
    fi
}

check_fail() {
    local desc="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        fail "$desc (expected failure)"
    else
        pass "$desc"
    fi
}

# --- Setup ---

log "Building"
cargo build --quiet 2>/dev/null
export PATH="$PWD/target/debug:$PATH"

log "Starting server on port $PORT"
B0_DB_PATH="$DB_PATH" $B0 server --port "$PORT" &
SERVER_PID=$!
sleep 2

# Server auto-configures CLI, so commands should work immediately.

# --- Test 1: Server start + basic operations ---

log "Test 1: Server start + basic worker operations"

$B0 worker add reviewer --description "Code reviewer" --instructions "Answer in one word."
check "worker add" $B0 worker ls

THREAD=$($B0 delegate reviewer "Capital of France?")
check "delegate returns thread ID" test -n "$THREAD"

$B0 wait
check "wait completes" true

$B0 worker remove reviewer
check "worker remove" true

# --- Test 2: Multi-turn conversation ---

log "Test 2: Multi-turn conversation"

$B0 worker add debater --description "Debater" --instructions "You are a debater. Max 2 sentences."

THREAD=$($B0 delegate debater "Argue that Python is the best language.")
$B0 wait

THREAD2=$($B0 delegate --thread "$THREAD" debater "I disagree. Rust is better. Counter my argument.")
check "multi-turn delegate" test -n "$THREAD2"
$B0 wait

$B0 worker remove debater

# --- Test 3: Worker temp ---

log "Test 3: Temporary worker"

THREAD=$($B0 worker temp "What is 2+2? Just the number.")
check "worker temp returns thread" test -n "$THREAD"
$B0 wait

# --- Test 4: Invite user + shared group ---

log "Test 4: User invitation and groups"

INVITE_OUTPUT=$($B0 invite alice 2>&1)
check "invite alice" echo "$INVITE_OUTPUT"

ALICE_KEY=$(echo "$INVITE_OUTPUT" | grep "Key:" | awk '{print $2}')
ALICE_ID=$(echo "$INVITE_OUTPUT" | grep "ID:" | sed 's/.*ID: //' | tr -d ')')

$B0 group create dev-team
check "group create" true

$B0 group add-member dev-team "$ALICE_ID"
check "group add-member" true

# --- Test 5: Skill install/uninstall ---

log "Test 5: Skill install/uninstall"

$B0 skill install claude-code
check "skill install claude-code" test -f ~/.claude/skills/b0/SKILL.md

$B0 skill uninstall claude-code
check "skill uninstall claude-code" test ! -d ~/.claude/skills/b0

# --- Test 6: Reset ---

log "Test 6: Reset"

# Stop server first, then reset
kill "$SERVER_PID" 2>/dev/null || true
wait "$SERVER_PID" 2>/dev/null || true
unset SERVER_PID

$B0 reset
check "reset completes" true

# --- Summary ---

echo ""
echo "=== Results ==="
echo "  Passed: $PASS"
echo "  Failed: $FAIL"
echo ""

if [ "$FAIL" -gt 0 ]; then
    exit 1
fi
