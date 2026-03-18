#!/bin/bash
#
# Stream0 Demo: Two agents coordinate a code review with mid-task dialogue
#
# Usage:
#   ./demo.sh                     # starts a local server and runs the demo
#   ./demo.sh http://yourserver   # runs against an existing server
#
set -e

URL="${1:-}"
STARTED_SERVER=false

if [ -z "$URL" ]; then
    echo "=== Building Stream0..."
    cargo build --release 2>/dev/null

    echo "=== Starting server on http://localhost:8080..."
    ./target/release/stream0 &>/dev/null &
    SERVER_PID=$!
    STARTED_SERVER=true
    URL="http://localhost:8080"
    sleep 1

    # Verify server is up
    if ! curl -sf "$URL/health" > /dev/null 2>&1; then
        echo "ERROR: Server failed to start"
        kill $SERVER_PID 2>/dev/null
        exit 1
    fi
fi

H="Content-Type: application/json"

echo ""
echo "================================================"
echo "  Stream0 Demo: Code Review with Mid-Task Q&A"
echo "================================================"
echo ""
echo "Two agents: an orchestrator assigns a code review,"
echo "the reviewer asks a question mid-task, gets an answer,"
echo "then completes the review."
echo ""

echo "--- Step 1: Register agents ---"
curl -s -X POST "$URL/agents" -H "$H" -d '{"id": "orchestrator"}' | python3 -m json.tool 2>/dev/null || echo '{"id": "orchestrator"}'
curl -s -X POST "$URL/agents" -H "$H" -d '{"id": "reviewer", "aliases": ["code-reviewer"]}' | python3 -m json.tool 2>/dev/null || echo '{"id": "reviewer"}'
echo ""

echo "--- Step 2: Orchestrator sends code review task ---"
curl -s -X POST "$URL/agents/reviewer/inbox" -H "$H" -d '{
  "task_id": "review-pr-42",
  "from": "orchestrator",
  "type": "request",
  "content": {
    "pr_url": "https://github.com/acme/app/pull/42",
    "files": ["auth.rs", "config.rs"],
    "priority": "high"
  }
}' | python3 -m json.tool 2>/dev/null || echo "(sent)"
echo ""

echo "--- Step 3: Reviewer picks up the task ---"
MESSAGES=$(curl -s "$URL/agents/reviewer/inbox?status=unread&task_id=review-pr-42")
echo "$MESSAGES" | python3 -m json.tool 2>/dev/null || echo "$MESSAGES"
MSG_ID=$(echo "$MESSAGES" | python3 -c "import sys,json; print(json.load(sys.stdin)['messages'][0]['id'])" 2>/dev/null || echo "")
if [ -n "$MSG_ID" ]; then
    curl -s -X POST "$URL/inbox/messages/$MSG_ID/ack" > /dev/null
fi
echo ""

echo "--- Step 4: Reviewer asks a clarifying question ---"
echo "    (This is the key feature — mid-task dialogue)"
curl -s -X POST "$URL/agents/orchestrator/inbox" -H "$H" -d '{
  "task_id": "review-pr-42",
  "from": "reviewer",
  "type": "question",
  "content": {
    "question": "auth.rs line 42 shadows a variable from outer scope. Intentional or bug?",
    "file": "auth.rs",
    "line": 42
  }
}' | python3 -m json.tool 2>/dev/null || echo "(sent)"
echo ""

echo "--- Step 5: Orchestrator answers ---"
Q_MSG=$(curl -s "$URL/agents/orchestrator/inbox?status=unread&task_id=review-pr-42")
Q_ID=$(echo "$Q_MSG" | python3 -c "import sys,json; print(json.load(sys.stdin)['messages'][0]['id'])" 2>/dev/null || echo "")
if [ -n "$Q_ID" ]; then
    curl -s -X POST "$URL/inbox/messages/$Q_ID/ack" > /dev/null
fi
curl -s -X POST "$URL/agents/reviewer/inbox" -H "$H" -d '{
  "task_id": "review-pr-42",
  "from": "orchestrator",
  "type": "answer",
  "content": {"answer": "Intentional — it is a test override. Safe to approve."}
}' | python3 -m json.tool 2>/dev/null || echo "(sent)"
echo ""

echo "--- Step 6: Reviewer completes the review ---"
A_MSG=$(curl -s "$URL/agents/reviewer/inbox?status=unread&task_id=review-pr-42")
A_ID=$(echo "$A_MSG" | python3 -c "import sys,json; print(json.load(sys.stdin)['messages'][0]['id'])" 2>/dev/null || echo "")
if [ -n "$A_ID" ]; then
    curl -s -X POST "$URL/inbox/messages/$A_ID/ack" > /dev/null
fi
curl -s -X POST "$URL/agents/orchestrator/inbox" -H "$H" -d '{
  "task_id": "review-pr-42",
  "from": "reviewer",
  "type": "done",
  "content": {
    "approved": true,
    "summary": "PR looks good. Variable shadow in auth.rs is intentional (confirmed)."
  }
}' | python3 -m json.tool 2>/dev/null || echo "(sent)"
echo ""

echo "================================================"
echo "  Full conversation history"
echo "================================================"
echo ""
curl -s "$URL/tasks/review-pr-42/messages" | python3 -m json.tool 2>/dev/null || curl -s "$URL/tasks/review-pr-42/messages"
echo ""

echo "================================================"
echo "  Demo complete!"
echo ""
echo "  4 messages, 1 task_id, 2 agents."
echo "  The reviewer asked a question mid-task,"
echo "  got an answer, and made the right decision."
echo "================================================"

if [ "$STARTED_SERVER" = true ]; then
    kill $SERVER_PID 2>/dev/null
    rm -f stream0.db
fi
