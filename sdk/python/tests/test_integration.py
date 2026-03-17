"""Integration tests that run against a real stream0 server.

To run these tests:
    1. Start stream0: ./stream0 --config stream0.yaml
    2. Set STREAM0_URL: export STREAM0_URL=http://localhost:8080
    3. Optionally set STREAM0_API_KEY if auth is enabled
    4. Run: pytest tests/test_integration.py -v

These tests are skipped if STREAM0_URL is not set.
"""

import os
import threading
import time
import uuid

import pytest

from stream0 import Stream0Client, Agent, TimeoutError

STREAM0_URL = os.environ.get("STREAM0_URL")
STREAM0_API_KEY = os.environ.get("STREAM0_API_KEY")

pytestmark = pytest.mark.skipif(
    not STREAM0_URL, reason="STREAM0_URL not set - skipping integration tests"
)


def unique_name(prefix="test"):
    """Generate a unique topic name to avoid test interference."""
    return f"{prefix}-{uuid.uuid4().hex[:8]}"


@pytest.fixture
def client():
    c = Stream0Client(STREAM0_URL, api_key=STREAM0_API_KEY)
    yield c
    c.close()


# --- Health ---


def test_health(client):
    result = client.health()
    assert result["status"] == "healthy"


# --- Topics ---


def test_create_and_get_topic(client):
    name = unique_name("topic")
    topic = client.create_topic(name)
    assert topic["name"] == name
    assert topic["retention_days"] == 7

    fetched = client.get_topic(name)
    assert fetched["name"] == name


def test_create_topic_idempotent(client):
    name = unique_name("topic")
    t1 = client.create_topic(name)
    t2 = client.create_topic(name)
    assert t1["id"] == t2["id"]


def test_list_topics(client):
    name = unique_name("topic")
    client.create_topic(name)
    topics = client.list_topics()
    names = [t["name"] for t in topics]
    assert name in names


# --- Publish & Consume ---


def test_publish_and_consume(client):
    topic = unique_name("pubsub")
    group = unique_name("group")
    client.create_topic(topic)

    # Publish
    result = client.publish(topic, {"text": "hello"}, headers={"trace": "123"})
    assert result["message_id"].startswith("msg-")
    assert result["offset"] == 1

    # Consume
    messages = client.consume(topic, group, timeout=5)
    assert len(messages) == 1
    assert messages[0]["payload"]["text"] == "hello"
    assert messages[0]["headers"]["trace"] == "123"

    # Ack
    ack_result = client.ack(messages[0]["id"], group)
    assert ack_result["status"] == "acknowledged"

    # No more messages
    messages = client.consume(topic, group, timeout=0.5)
    assert len(messages) == 0


def test_publish_multiple_consume_in_order(client):
    topic = unique_name("order")
    group = unique_name("group")
    client.create_topic(topic)

    for i in range(5):
        client.publish(topic, {"n": i})

    messages = client.consume(topic, group, max_messages=5, timeout=5)
    assert len(messages) == 5

    for i, msg in enumerate(messages):
        assert msg["payload"]["n"] == i
        assert msg["offset"] == i + 1


def test_consume_max_messages(client):
    topic = unique_name("maxmsg")
    group = unique_name("group")
    client.create_topic(topic)

    for i in range(10):
        client.publish(topic, {"n": i})

    messages = client.consume(topic, group, max_messages=3, timeout=5)
    assert len(messages) == 3


def test_consumer_group_isolation(client):
    topic = unique_name("isolation")
    group1 = unique_name("group")
    group2 = unique_name("group")
    client.create_topic(topic)

    client.publish(topic, {"data": "shared"})

    # Both groups should get the message
    msgs1 = client.consume(topic, group1, timeout=5)
    msgs2 = client.consume(topic, group2, timeout=5)
    assert len(msgs1) == 1
    assert len(msgs2) == 1


def test_visibility_timeout(client):
    topic = unique_name("visibility")
    group = unique_name("group")
    client.create_topic(topic)

    client.publish(topic, {"task": "retry-me"})

    # Consume with short visibility timeout (5s minimum)
    msgs = client.consume(topic, group, visibility_timeout=5, timeout=5)
    assert len(msgs) == 1

    # Don't ack - message should not be immediately available
    msgs2 = client.consume(topic, group, timeout=0.5)
    assert len(msgs2) == 0

    # Wait for visibility timeout to expire
    time.sleep(6)

    # Message should be available again
    msgs3 = client.consume(topic, group, timeout=5)
    assert len(msgs3) == 1
    assert msgs3[0]["delivery_count"] > msgs[0]["delivery_count"]


# --- Request-Reply ---


def test_request_reply(client):
    topic = unique_name("rr")
    client.create_topic(topic)

    result_holder = {}

    def requester():
        try:
            result = client.request(topic, {"question": "2+2?"}, timeout=15)
            result_holder["result"] = result
        except Exception as e:
            result_holder["error"] = e

    # Start requester in background
    t = threading.Thread(target=requester)
    t.start()

    # Give time for the request to be published
    time.sleep(0.5)

    # Responder: consume the request
    messages = client.consume(topic, "responders", timeout=10)
    assert len(messages) == 1
    assert messages[0]["payload"]["question"] == "2+2?"
    assert "correlation_id" in messages[0]["headers"]

    # Send reply
    reply_result = client.reply(
        messages[0]["id"],
        {"answer": 4},
        group="responders",
    )
    assert reply_result["status"] == "reply sent"

    # Wait for requester to finish
    t.join(timeout=20)

    assert "error" not in result_holder, f"requester failed: {result_holder.get('error')}"
    assert "result" in result_holder

    result = result_holder["result"]
    assert result["reply"]["payload"]["answer"] == 4
    assert result["correlation_id"] is not None


def test_request_reply_timeout(client):
    topic = unique_name("rr-timeout")
    client.create_topic(topic)

    with pytest.raises(TimeoutError):
        client.request(topic, {"question": "hello?"}, timeout=1)


def test_request_reply_with_headers(client):
    topic = unique_name("rr-headers")
    client.create_topic(topic)

    result_holder = {}

    def requester():
        try:
            result = client.request(
                topic,
                {"q": "test"},
                headers={"priority": "high"},
                timeout=15,
            )
            result_holder["result"] = result
        except Exception as e:
            result_holder["error"] = e

    t = threading.Thread(target=requester)
    t.start()
    time.sleep(0.5)

    messages = client.consume(topic, "workers", timeout=10)
    assert len(messages) == 1
    # Custom headers should be preserved alongside correlation_id
    assert messages[0]["headers"]["priority"] == "high"
    assert "correlation_id" in messages[0]["headers"]

    client.reply(messages[0]["id"], {"result": "done"}, group="workers")
    t.join(timeout=20)

    assert "error" not in result_holder


def test_multiple_concurrent_request_reply(client):
    topic = unique_name("rr-concurrent")
    client.create_topic(topic)

    num_requests = 3
    results = [None] * num_requests
    errors = [None] * num_requests

    def requester(idx):
        try:
            result = client.request(
                topic,
                {"question": f"q{idx}"},
                timeout=20,
            )
            results[idx] = result
        except Exception as e:
            errors[idx] = e

    # Launch requesters
    threads = []
    for i in range(num_requests):
        t = threading.Thread(target=requester, args=(i,))
        t.start()
        threads.append(t)

    time.sleep(1)

    # Responder: consume and reply to all
    for _ in range(num_requests):
        messages = client.consume(topic, "workers", max_messages=1, timeout=10)
        if messages:
            client.reply(
                messages[0]["id"],
                {"answer": f"a-{messages[0]['payload']['question']}"},
                group="workers",
            )

    for t in threads:
        t.join(timeout=25)

    for i in range(num_requests):
        assert errors[i] is None, f"request {i} failed: {errors[i]}"
        assert results[i] is not None, f"request {i} got no result"
        assert results[i]["reply"]["payload"]["answer"].startswith("a-")


# --- Full workflow ---


def test_full_agent_workflow(client):
    """Simulates two agents communicating through stream0."""
    task_topic = unique_name("agent-tasks")
    client.create_topic(task_topic)

    # Agent A: sends a task request
    result_holder = {}

    def agent_a():
        try:
            result = client.request(
                task_topic,
                {
                    "task": "summarize",
                    "text": "The quick brown fox jumps over the lazy dog.",
                },
                timeout=15,
            )
            result_holder["result"] = result
        except Exception as e:
            result_holder["error"] = e

    t = threading.Thread(target=agent_a)
    t.start()
    time.sleep(0.5)

    # Agent B: picks up the task, processes it, replies
    messages = client.consume(task_topic, "agent-b-group", timeout=10)
    assert len(messages) == 1

    task = messages[0]
    assert task["payload"]["task"] == "summarize"

    # "Process" the task
    summary = f"Summary: {len(task['payload']['text'].split())} words"

    # Reply
    client.reply(
        task["id"],
        {"summary": summary, "status": "completed"},
        group="agent-b-group",
    )

    t.join(timeout=20)

    assert "error" not in result_holder
    result = result_holder["result"]
    assert result["reply"]["payload"]["status"] == "completed"
    assert "9 words" in result["reply"]["payload"]["summary"]


# --- v2 Inbox Model Integration Tests ---


@pytest.fixture
def main_agent():
    agent_id = unique_name("main")
    a = Agent(agent_id, url=STREAM0_URL, api_key=STREAM0_API_KEY)
    a.register()
    yield a
    a.close()


@pytest.fixture
def worker_agent():
    agent_id = unique_name("worker")
    a = Agent(agent_id, url=STREAM0_URL, api_key=STREAM0_API_KEY)
    a.register()
    yield a
    a.close()


def test_list_agents_integration(client):
    # Register a few agents with unique names
    a1 = unique_name("agent")
    a2 = unique_name("agent")
    client.register_agent(a1)
    client.register_agent(a2)

    agents = client.list_agents()
    agent_ids = [a["id"] for a in agents]
    assert a1 in agent_ids
    assert a2 in agent_ids


def test_list_agents_after_delete(client):
    agent_id = unique_name("agent")
    client.register_agent(agent_id)

    # Verify it's in the list
    agents = client.list_agents()
    assert agent_id in [a["id"] for a in agents]

    # Delete and verify it's gone
    client.delete_agent(agent_id)
    agents = client.list_agents()
    assert agent_id not in [a["id"] for a in agents]


def test_register_agent_integration(client):
    agent_id = unique_name("agent")
    result = client.register_agent(agent_id)
    assert result["id"] == agent_id


def test_register_agent_idempotent(client):
    agent_id = unique_name("agent")
    r1 = client.register_agent(agent_id)
    r2 = client.register_agent(agent_id)
    assert r1["id"] == r2["id"]


def test_delete_agent_integration(client):
    agent_id = unique_name("agent")
    client.register_agent(agent_id)
    result = client.delete_agent(agent_id)
    assert result["status"] == "deleted"


def test_send_and_receive(main_agent, worker_agent):
    task_id = unique_name("task")

    # Main sends to worker
    main_agent.send(worker_agent.agent_id, task_id=task_id, msg_type="request",
                    content={"instruction": "process this"})

    # Worker receives
    messages = worker_agent.receive(task_id=task_id)
    assert len(messages) == 1
    assert messages[0]["task_id"] == task_id
    assert messages[0]["type"] == "request"
    assert messages[0]["content"]["instruction"] == "process this"


def test_ack_marks_as_read(main_agent, worker_agent):
    task_id = unique_name("task")

    main_agent.send(worker_agent.agent_id, task_id=task_id, msg_type="request")

    # Get the message
    messages = worker_agent.receive()
    assert len(messages) == 1

    # Ack it
    worker_agent.ack(messages[0]["id"])

    # Should no longer appear in unread
    unread = worker_agent.receive()
    assert len(unread) == 0


def test_inbox_long_polling(main_agent, worker_agent):
    task_id = unique_name("task")

    result_holder = {}

    def poller():
        messages = worker_agent.receive(task_id=task_id, timeout=10)
        result_holder["messages"] = messages

    # Start long-polling
    t = threading.Thread(target=poller)
    t.start()

    # Wait a bit, then send
    time.sleep(1)
    main_agent.send(worker_agent.agent_id, task_id=task_id, msg_type="request",
                    content={"data": "arrived via long-poll"})

    t.join(timeout=15)

    assert "messages" in result_holder
    assert len(result_holder["messages"]) == 1
    assert result_holder["messages"][0]["content"]["data"] == "arrived via long-poll"


def test_task_history(main_agent, worker_agent):
    task_id = unique_name("task")

    # Send request
    main_agent.send(worker_agent.agent_id, task_id=task_id, msg_type="request",
                    content={"instruction": "translate"})

    # Worker asks question
    worker_agent.send(main_agent.agent_id, task_id=task_id, msg_type="question",
                      content={"q": "A or B?"})

    # Main answers
    main_agent.send(worker_agent.agent_id, task_id=task_id, msg_type="answer",
                    content={"a": "use A"})

    # Worker completes
    worker_agent.send(main_agent.agent_id, task_id=task_id, msg_type="done",
                      content={"result": "translated document"})

    # Get full history
    history = main_agent.history(task_id)
    assert len(history) == 4
    assert [m["type"] for m in history] == ["request", "question", "answer", "done"]


def test_multi_turn_translation_scenario(main_agent, worker_agent):
    """Full translation scenario from the PRD."""
    task_id = unique_name("translate")

    # Step 1: Main agent sends translation task
    main_agent.send(worker_agent.agent_id, task_id=task_id, msg_type="request",
                    content={
                        "instruction": "Translate this legal contract to Japanese",
                        "document": "The party of the first part hereby...",
                    })

    # Step 2: Worker picks up the task
    messages = worker_agent.receive(task_id=task_id)
    assert len(messages) == 1
    assert messages[0]["type"] == "request"
    worker_agent.ack(messages[0]["id"])

    # Step 3: Worker finds ambiguity, asks a question
    worker_agent.send(main_agent.agent_id, task_id=task_id, msg_type="question",
                      content={
                          "question": "Clause 3 uses 'indemnification' - use 損害賠償 or 補償?",
                      })

    # Step 4: Main agent receives the question
    questions = main_agent.receive(task_id=task_id)
    assert len(questions) == 1
    assert questions[0]["type"] == "question"
    main_agent.ack(questions[0]["id"])

    # Step 5: Main agent answers
    main_agent.send(worker_agent.agent_id, task_id=task_id, msg_type="answer",
                    content={"answer": "Use 補償 (compensation)"})

    # Step 6: Worker receives answer, continues, completes
    answers = worker_agent.receive(task_id=task_id)
    assert len(answers) == 1
    assert answers[0]["type"] == "answer"
    worker_agent.ack(answers[0]["id"])

    # Step 7: Worker sends completed result
    worker_agent.send(main_agent.agent_id, task_id=task_id, msg_type="done",
                      content={"translated_document": "第一当事者は、ここに..."})

    # Step 8: Main agent receives the result
    results = main_agent.receive(task_id=task_id)
    assert len(results) == 1
    assert results[0]["type"] == "done"
    assert "第一当事者" in results[0]["content"]["translated_document"]

    # Verify full conversation history
    history = main_agent.history(task_id)
    assert len(history) == 4

    expected_flow = [
        ("request", main_agent.agent_id, worker_agent.agent_id),
        ("question", worker_agent.agent_id, main_agent.agent_id),
        ("answer", main_agent.agent_id, worker_agent.agent_id),
        ("done", worker_agent.agent_id, main_agent.agent_id),
    ]

    for i, (exp_type, exp_from, exp_to) in enumerate(expected_flow):
        assert history[i]["type"] == exp_type, f"msg {i}: expected type {exp_type}, got {history[i]['type']}"
        assert history[i]["from"] == exp_from, f"msg {i}: expected from {exp_from}, got {history[i]['from']}"
        assert history[i]["to"] == exp_to, f"msg {i}: expected to {exp_to}, got {history[i]['to']}"


def test_multiple_sub_agents(client):
    """Main agent manages 3 sub-agents concurrently, all on the same task."""
    main_id = unique_name("main")
    research_id = unique_name("research")
    writer_id = unique_name("writer")
    charts_id = unique_name("charts")
    task_id = unique_name("report")

    # Register all agents
    for agent_id in [main_id, research_id, writer_id, charts_id]:
        client.register_agent(agent_id)

    # Main sends tasks to all 3
    for sub_id, instruction in [
        (research_id, "find market data"),
        (writer_id, "write executive summary"),
        (charts_id, "create visualizations"),
    ]:
        client.send(sub_id, task_id, main_id, "request", {"instruction": instruction})

    # Each sub-agent completes
    for sub_id, result in [
        (research_id, {"data": "market is $5B"}),
        (writer_id, {"summary": "Report written"}),
        (charts_id, {"chart": "chart.png"}),
    ]:
        client.send(main_id, task_id, sub_id, "done", result)

    # Main sees all 3 completions
    messages = client.receive(main_id, task_id=task_id)
    assert len(messages) == 3
    assert all(m["type"] == "done" for m in messages)

    # Full task history: 3 requests + 3 completions = 6
    history = client.get_task_messages(task_id)
    assert len(history) == 6


def test_inbox_isolation(client):
    """Messages to agent A don't appear in agent B's inbox."""
    agent_a = unique_name("agent")
    agent_b = unique_name("agent")
    client.register_agent(agent_a)
    client.register_agent(agent_b)

    client.send(agent_a, "task-1", "sender", "request", {"for": "a"})
    client.send(agent_b, "task-1", "sender", "request", {"for": "b"})

    msgs_a = client.receive(agent_a)
    msgs_b = client.receive(agent_b)

    assert len(msgs_a) == 1
    assert len(msgs_b) == 1
    assert msgs_a[0]["content"]["for"] == "a"
    assert msgs_b[0]["content"]["for"] == "b"


def test_failed_task(main_agent, worker_agent):
    """Worker reports task failure."""
    task_id = unique_name("task")

    main_agent.send(worker_agent.agent_id, task_id=task_id, msg_type="request",
                    content={"instruction": "do something impossible"})

    messages = worker_agent.receive(task_id=task_id)
    worker_agent.ack(messages[0]["id"])

    # Worker fails
    worker_agent.send(main_agent.agent_id, task_id=task_id, msg_type="failed",
                      content={"error": "task is impossible", "code": "IMPOSSIBLE"})

    # Main sees the failure
    results = main_agent.receive(task_id=task_id)
    assert len(results) == 1
    assert results[0]["type"] == "failed"
    assert results[0]["content"]["error"] == "task is impossible"
