"""Unit tests for Stream0Client using mocked HTTP responses."""

import pytest
import responses

from stream0 import (
    Stream0Client,
    AuthenticationError,
    NotFoundError,
    TimeoutError,
    ServerError,
    Stream0Error,
)

BASE_URL = "http://localhost:8080"


@pytest.fixture
def client():
    c = Stream0Client(BASE_URL)
    yield c
    c.close()


@pytest.fixture
def auth_client():
    c = Stream0Client(BASE_URL, api_key="test-key-123")
    yield c
    c.close()


# --- Health ---


@responses.activate
def test_health(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/health",
        json={"status": "healthy", "version": "0.1.0-go"},
        status=200,
    )
    result = client.health()
    assert result["status"] == "healthy"
    assert result["version"] == "0.1.0-go"


# --- Topics ---


@responses.activate
def test_create_topic(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics",
        json={"id": 1, "name": "events", "retention_days": 7, "message_count": 0},
        status=201,
    )
    result = client.create_topic("events")
    assert result["name"] == "events"
    assert result["retention_days"] == 7

    # Verify request body
    body = responses.calls[0].request.body
    assert b'"name": "events"' in body or b'"name":"events"' in body


@responses.activate
def test_create_topic_custom_retention(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics",
        json={"id": 1, "name": "logs", "retention_days": 30, "message_count": 0},
        status=201,
    )
    result = client.create_topic("logs", retention_days=30)
    assert result["retention_days"] == 30


@responses.activate
def test_list_topics(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics",
        json=[
            {"id": 1, "name": "alpha", "retention_days": 7, "message_count": 5},
            {"id": 2, "name": "beta", "retention_days": 14, "message_count": 0},
        ],
        status=200,
    )
    result = client.list_topics()
    assert len(result) == 2
    assert result[0]["name"] == "alpha"


@responses.activate
def test_get_topic(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics/events",
        json={"id": 1, "name": "events", "retention_days": 7, "message_count": 42},
        status=200,
    )
    result = client.get_topic("events")
    assert result["name"] == "events"
    assert result["message_count"] == 42


@responses.activate
def test_get_topic_not_found(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics/ghost",
        json={"error": "Topic not found"},
        status=404,
    )
    with pytest.raises(NotFoundError) as exc_info:
        client.get_topic("ghost")
    assert exc_info.value.status_code == 404


# --- Publish ---


@responses.activate
def test_publish(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics/events/messages",
        json={"message_id": "msg-abc123", "offset": 1, "timestamp": "2024-01-01T00:00:00Z"},
        status=201,
    )
    result = client.publish("events", {"action": "click"})
    assert result["message_id"] == "msg-abc123"
    assert result["offset"] == 1


@responses.activate
def test_publish_with_headers(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics/events/messages",
        json={"message_id": "msg-abc123", "offset": 1, "timestamp": "2024-01-01T00:00:00Z"},
        status=201,
    )
    result = client.publish("events", {"data": "value"}, headers={"trace-id": "xyz"})
    assert result["message_id"] is not None


@responses.activate
def test_publish_to_nonexistent_topic(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics/ghost/messages",
        json={"error": "Topic not found"},
        status=404,
    )
    with pytest.raises(NotFoundError):
        client.publish("ghost", {"x": 1})


# --- Consume ---


@responses.activate
def test_consume(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics/events/messages",
        json={
            "messages": [
                {
                    "id": "msg-1",
                    "offset": 1,
                    "payload": {"n": 1},
                    "headers": {},
                    "delivery_count": 1,
                },
                {
                    "id": "msg-2",
                    "offset": 2,
                    "payload": {"n": 2},
                    "headers": {},
                    "delivery_count": 1,
                },
            ]
        },
        status=200,
    )
    messages = client.consume("events", "group1", timeout=1)
    assert len(messages) == 2
    assert messages[0]["id"] == "msg-1"
    assert messages[1]["payload"]["n"] == 2


@responses.activate
def test_consume_empty(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics/events/messages",
        json={"messages": []},
        status=200,
    )
    messages = client.consume("events", "group1", timeout=0.1)
    assert messages == []


@responses.activate
def test_consume_query_params(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics/events/messages",
        json={"messages": []},
        status=200,
    )
    client.consume("events", "workers", max_messages=5, timeout=10, visibility_timeout=60)

    # Verify query parameters
    request_url = responses.calls[0].request.url
    assert "group=workers" in request_url
    assert "max=5" in request_url
    assert "timeout=10" in request_url
    assert "visibility_timeout=60" in request_url


# --- Ack ---


@responses.activate
def test_ack(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/messages/msg-abc/ack",
        json={"status": "acknowledged", "message_id": "msg-abc"},
        status=200,
    )
    result = client.ack("msg-abc", "group1")
    assert result["status"] == "acknowledged"


@responses.activate
def test_ack_not_found(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/messages/msg-ghost/ack",
        json={"error": "message not found or not leased by this group"},
        status=404,
    )
    with pytest.raises(NotFoundError):
        client.ack("msg-ghost", "group1")


# --- Request-Reply ---


@responses.activate
def test_request(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics/tasks/request",
        json={
            "request_id": "msg-req1",
            "correlation_id": "corr-abc",
            "reply": {
                "correlation_id": "corr-abc",
                "payload": {"answer": 42},
                "headers": {"correlation_id": "corr-abc"},
            },
        },
        status=200,
    )
    result = client.request("tasks", {"question": "what?"}, timeout=5)
    assert result["correlation_id"] == "corr-abc"
    assert result["reply"]["payload"]["answer"] == 42


@responses.activate
def test_request_timeout(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics/tasks/request",
        json={
            "error": "request timed out waiting for reply",
            "request_id": "msg-req1",
            "correlation_id": "corr-abc",
        },
        status=504,
    )
    with pytest.raises(TimeoutError) as exc_info:
        client.request("tasks", {"q": "test"}, timeout=1)
    assert exc_info.value.status_code == 504


@responses.activate
def test_request_topic_not_found(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics/ghost/request",
        json={"error": "Topic not found"},
        status=404,
    )
    with pytest.raises(NotFoundError):
        client.request("ghost", {"q": "test"})


@responses.activate
def test_reply(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/messages/msg-req1/reply",
        json={
            "status": "reply sent",
            "correlation_id": "corr-abc",
            "message_id": "msg-req1",
        },
        status=200,
    )
    result = client.reply("msg-req1", {"answer": 42})
    assert result["status"] == "reply sent"
    assert result["correlation_id"] == "corr-abc"


@responses.activate
def test_reply_with_ack(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/messages/msg-req1/reply",
        json={
            "status": "reply sent",
            "correlation_id": "corr-abc",
            "message_id": "msg-req1",
        },
        status=200,
    )
    result = client.reply("msg-req1", {"answer": 42}, group="workers")
    assert result["status"] == "reply sent"

    # Verify group was sent in body
    import json
    body = json.loads(responses.calls[0].request.body)
    assert body["group"] == "workers"


@responses.activate
def test_reply_message_not_found(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/messages/msg-ghost/reply",
        json={"error": "message not found"},
        status=404,
    )
    with pytest.raises(NotFoundError):
        client.reply("msg-ghost", {"x": 1})


# --- Authentication ---


@responses.activate
def test_auth_header_sent(auth_client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics",
        json=[],
        status=200,
    )
    auth_client.list_topics()

    # Verify API key header was sent
    assert responses.calls[0].request.headers["X-API-Key"] == "test-key-123"


@responses.activate
def test_auth_failure(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics",
        json={"error": "missing X-API-Key header"},
        status=401,
    )
    with pytest.raises(AuthenticationError) as exc_info:
        client.list_topics()
    assert exc_info.value.status_code == 401


# --- Error handling ---


@responses.activate
def test_server_error(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/topics",
        json={"error": "internal server error"},
        status=500,
    )
    with pytest.raises(ServerError) as exc_info:
        client.list_topics()
    assert exc_info.value.status_code == 500


@responses.activate
def test_bad_request(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/topics",
        json={"error": "name is required"},
        status=400,
    )
    with pytest.raises(Stream0Error) as exc_info:
        client.create_topic("")
    assert exc_info.value.status_code == 400


# --- Context manager ---


@responses.activate
def test_context_manager():
    responses.add(
        responses.GET,
        f"{BASE_URL}/health",
        json={"status": "healthy"},
        status=200,
    )
    with Stream0Client(BASE_URL) as client:
        result = client.health()
        assert result["status"] == "healthy"


# --- URL handling ---


def test_trailing_slash_stripped():
    client = Stream0Client("http://localhost:8080/")
    assert client.base_url == "http://localhost:8080"
    client.close()


def test_url_construction():
    client = Stream0Client("http://example.com:9090")
    assert client._url("/topics") == "http://example.com:9090/topics"
    client.close()


# --- v2 Inbox Model Tests ---


from stream0 import Agent


@responses.activate
def test_register_agent(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents",
        json={"id": "agent-1", "created_at": "2024-01-01T00:00:00Z"},
        status=201,
    )
    result = client.register_agent("agent-1")
    assert result["id"] == "agent-1"

    import json
    body = json.loads(responses.calls[0].request.body)
    assert body["id"] == "agent-1"


@responses.activate
def test_delete_agent(client):
    responses.add(
        responses.DELETE,
        f"{BASE_URL}/agents/agent-1",
        json={"status": "deleted", "agent_id": "agent-1"},
        status=200,
    )
    result = client.delete_agent("agent-1")
    assert result["status"] == "deleted"


@responses.activate
def test_delete_agent_not_found(client):
    responses.add(
        responses.DELETE,
        f"{BASE_URL}/agents/ghost",
        json={"error": "agent not found"},
        status=404,
    )
    with pytest.raises(NotFoundError):
        client.delete_agent("ghost")


@responses.activate
def test_send_message(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents/worker/inbox",
        json={"message_id": "imsg-abc123", "created_at": "2024-01-01T00:00:00Z"},
        status=201,
    )
    result = client.send(
        to="worker",
        task_id="task-1",
        from_agent="main",
        msg_type="request",
        content={"instruction": "do work"},
    )
    assert result["message_id"] == "imsg-abc123"

    import json
    body = json.loads(responses.calls[0].request.body)
    assert body["task_id"] == "task-1"
    assert body["from"] == "main"
    assert body["type"] == "request"
    assert body["content"]["instruction"] == "do work"


@responses.activate
def test_send_message_agent_not_found(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents/ghost/inbox",
        json={"error": "agent not found"},
        status=404,
    )
    with pytest.raises(NotFoundError):
        client.send("ghost", "task-1", "main", "request")


@responses.activate
def test_receive_messages(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/agents/worker/inbox",
        json={
            "messages": [
                {
                    "id": "imsg-1",
                    "task_id": "task-1",
                    "from": "main",
                    "to": "worker",
                    "type": "request",
                    "content": {"n": 1},
                    "status": "unread",
                },
            ]
        },
        status=200,
    )
    messages = client.receive("worker", status="unread")
    assert len(messages) == 1
    assert messages[0]["task_id"] == "task-1"

    request_url = responses.calls[0].request.url
    assert "status=unread" in request_url


@responses.activate
def test_receive_with_task_filter(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/agents/worker/inbox",
        json={"messages": []},
        status=200,
    )
    client.receive("worker", task_id="task-42")

    request_url = responses.calls[0].request.url
    assert "task_id=task-42" in request_url


@responses.activate
def test_receive_with_long_poll(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/agents/worker/inbox",
        json={"messages": []},
        status=200,
    )
    client.receive("worker", timeout=10)

    request_url = responses.calls[0].request.url
    assert "timeout=10" in request_url


@responses.activate
def test_receive_empty(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/agents/worker/inbox",
        json={"messages": []},
        status=200,
    )
    messages = client.receive("worker")
    assert messages == []


@responses.activate
def test_ack_inbox_message(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/inbox/messages/imsg-abc/ack",
        json={"status": "acked", "message_id": "imsg-abc"},
        status=200,
    )
    result = client.ack_inbox("imsg-abc")
    assert result["status"] == "acked"


@responses.activate
def test_ack_inbox_message_not_found(client):
    responses.add(
        responses.POST,
        f"{BASE_URL}/inbox/messages/imsg-ghost/ack",
        json={"error": "message not found or already acked"},
        status=404,
    )
    with pytest.raises(NotFoundError):
        client.ack_inbox("imsg-ghost")


@responses.activate
def test_get_task_messages(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/tasks/task-1/messages",
        json={
            "messages": [
                {"id": "imsg-1", "task_id": "task-1", "from": "main", "to": "worker", "type": "request"},
                {"id": "imsg-2", "task_id": "task-1", "from": "worker", "to": "main", "type": "question"},
                {"id": "imsg-3", "task_id": "task-1", "from": "main", "to": "worker", "type": "answer"},
                {"id": "imsg-4", "task_id": "task-1", "from": "worker", "to": "main", "type": "done"},
            ]
        },
        status=200,
    )
    messages = client.get_task_messages("task-1")
    assert len(messages) == 4
    assert [m["type"] for m in messages] == ["request", "question", "answer", "done"]


@responses.activate
def test_get_task_messages_empty(client):
    responses.add(
        responses.GET,
        f"{BASE_URL}/tasks/nonexistent/messages",
        json={"messages": []},
        status=200,
    )
    messages = client.get_task_messages("nonexistent")
    assert messages == []


# --- Agent high-level class ---


@responses.activate
def test_agent_register():
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents",
        json={"id": "my-agent", "created_at": "2024-01-01T00:00:00Z"},
        status=201,
    )
    with Agent("my-agent", url=BASE_URL) as agent:
        result = agent.register()
        assert result["id"] == "my-agent"


@responses.activate
def test_agent_send():
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents/other-agent/inbox",
        json={"message_id": "imsg-1", "created_at": "2024-01-01T00:00:00Z"},
        status=201,
    )
    with Agent("my-agent", url=BASE_URL) as agent:
        result = agent.send("other-agent", task_id="t1", msg_type="request", content={"x": 1})
        assert result["message_id"] == "imsg-1"

    # Verify from field is set to agent_id
    import json
    body = json.loads(responses.calls[0].request.body)
    assert body["from"] == "my-agent"


@responses.activate
def test_agent_receive():
    responses.add(
        responses.GET,
        f"{BASE_URL}/agents/my-agent/inbox",
        json={
            "messages": [
                {"id": "imsg-1", "task_id": "t1", "from": "other", "type": "request", "status": "unread"},
            ]
        },
        status=200,
    )
    with Agent("my-agent", url=BASE_URL) as agent:
        messages = agent.receive()
        assert len(messages) == 1

    # Verify status=unread is the default filter
    request_url = responses.calls[0].request.url
    assert "status=unread" in request_url


@responses.activate
def test_agent_ack():
    responses.add(
        responses.POST,
        f"{BASE_URL}/inbox/messages/imsg-1/ack",
        json={"status": "acked", "message_id": "imsg-1"},
        status=200,
    )
    with Agent("my-agent", url=BASE_URL) as agent:
        result = agent.ack("imsg-1")
        assert result["status"] == "acked"


@responses.activate
def test_agent_history():
    responses.add(
        responses.GET,
        f"{BASE_URL}/tasks/t1/messages",
        json={
            "messages": [
                {"id": "imsg-1", "type": "request"},
                {"id": "imsg-2", "type": "done"},
            ]
        },
        status=200,
    )
    with Agent("my-agent", url=BASE_URL) as agent:
        messages = agent.history("t1")
        assert len(messages) == 2


@responses.activate
def test_agent_with_api_key():
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents",
        json={"id": "secure-agent", "created_at": "2024-01-01T00:00:00Z"},
        status=201,
    )
    with Agent("secure-agent", url=BASE_URL, api_key="secret-key") as agent:
        agent.register()

    assert responses.calls[0].request.headers["X-API-Key"] == "secret-key"


@responses.activate
def test_agent_full_conversation():
    """Test a complete multi-turn conversation using the Agent class."""
    # Register both agents
    responses.add(responses.POST, f"{BASE_URL}/agents", json={"id": "main"}, status=201)
    responses.add(responses.POST, f"{BASE_URL}/agents", json={"id": "translator"}, status=201)

    # Main sends request
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents/translator/inbox",
        json={"message_id": "imsg-1"},
        status=201,
    )

    # Translator receives
    responses.add(
        responses.GET,
        f"{BASE_URL}/agents/translator/inbox",
        json={"messages": [{"id": "imsg-1", "task_id": "t1", "type": "request", "from": "main"}]},
        status=200,
    )

    # Translator asks question
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents/main/inbox",
        json={"message_id": "imsg-2"},
        status=201,
    )

    # Main receives question
    responses.add(
        responses.GET,
        f"{BASE_URL}/agents/main/inbox",
        json={"messages": [{"id": "imsg-2", "task_id": "t1", "type": "question", "from": "translator"}]},
        status=200,
    )

    # Main answers
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents/translator/inbox",
        json={"message_id": "imsg-3"},
        status=201,
    )

    # Translator completes
    responses.add(
        responses.POST,
        f"{BASE_URL}/agents/main/inbox",
        json={"message_id": "imsg-4"},
        status=201,
    )

    # Full conversation history
    responses.add(
        responses.GET,
        f"{BASE_URL}/tasks/t1/messages",
        json={
            "messages": [
                {"id": "imsg-1", "type": "request", "from": "main", "to": "translator"},
                {"id": "imsg-2", "type": "question", "from": "translator", "to": "main"},
                {"id": "imsg-3", "type": "answer", "from": "main", "to": "translator"},
                {"id": "imsg-4", "type": "done", "from": "translator", "to": "main"},
            ]
        },
        status=200,
    )

    main = Agent("main", url=BASE_URL)
    translator = Agent("translator", url=BASE_URL)

    main.register()
    translator.register()

    # Main sends task
    main.send("translator", task_id="t1", msg_type="request", content={"text": "translate this"})

    # Translator picks up
    msgs = translator.receive(task_id="t1")
    assert len(msgs) == 1
    assert msgs[0]["type"] == "request"

    # Translator asks question
    translator.send("main", task_id="t1", msg_type="question", content={"q": "A or B?"})

    # Main answers
    msgs = main.receive(task_id="t1")
    assert msgs[0]["type"] == "question"
    main.send("translator", task_id="t1", msg_type="answer", content={"a": "use A"})

    # Translator completes
    translator.send("main", task_id="t1", msg_type="done", content={"result": "translated"})

    # Check full history
    history = main.history("t1")
    assert len(history) == 4
    assert [m["type"] for m in history] == ["request", "question", "answer", "done"]

    main.close()
    translator.close()
