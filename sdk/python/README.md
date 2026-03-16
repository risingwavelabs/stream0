# Stream0 Python SDK

Python client for [Stream0](https://github.com/risingwavelabs/stream0) — the communication layer for AI agents.

## Install

```bash
pip install -e .
```

## Usage

### Agent class (recommended)

```python
from stream0 import Agent

# Create and register an agent
agent = Agent("my-agent", url="http://localhost:8080", api_key="optional-key")
agent.register()

# Send a message to another agent
agent.send("other-agent", task_id="task-1", msg_type="request",
           content={"instruction": "do something"})

# Read inbox
messages = agent.receive()                    # all unread
messages = agent.receive(task_id="task-1")    # filter by task
messages = agent.receive(timeout=10)          # long-poll up to 10s

# Acknowledge a message
agent.ack(messages[0]["id"])

# Get full conversation history
history = agent.history("task-1")
```

### Full conversation example

```python
from stream0 import Agent

main = Agent("main-agent", url="http://localhost:8080")
worker = Agent("worker", url="http://localhost:8080")

main.register()
worker.register()

# Main sends task
main.send("worker", task_id="t1", msg_type="request",
          content={"instruction": "translate this contract"})

# Worker picks up
msgs = worker.receive(task_id="t1")
worker.ack(msgs[0]["id"])

# Worker asks a question
worker.send("main-agent", task_id="t1", msg_type="question",
            content={"q": "Use term A or B?"})

# Main answers
msgs = main.receive(task_id="t1")
main.ack(msgs[0]["id"])
main.send("worker", task_id="t1", msg_type="answer",
          content={"a": "use B"})

# Worker completes
msgs = worker.receive(task_id="t1")
worker.ack(msgs[0]["id"])
worker.send("main-agent", task_id="t1", msg_type="done",
            content={"result": "translated document"})

# View full conversation
history = main.history("t1")
# [request, question, answer, done]
```

### Low-level client

For direct API access without a fixed agent identity:

```python
from stream0 import Stream0Client

client = Stream0Client("http://localhost:8080", api_key="optional-key")

# v2 inbox API
client.register_agent("my-agent")
client.send("target-agent", "task-1", "my-agent", "request", {"data": "hello"})
messages = client.receive("my-agent", status="unread")
client.ack_inbox(messages[0]["id"])
history = client.get_task_messages("task-1")

# v1 topic API (still available)
client.create_topic("events")
client.publish("events", {"action": "click"})
messages = client.consume("events", "group1", timeout=5)
client.ack(messages[0]["id"], "group1")
```

## Message types

| Type | Use |
|------|-----|
| `request` | Start a task |
| `question` | Ask for clarification mid-task |
| `answer` | Respond to a question |
| `done` | Task completed |
| `failed` | Task failed |

## Error handling

```python
from stream0 import Agent, NotFoundError, AuthenticationError, TimeoutError

agent = Agent("my-agent", url="http://localhost:8080")

try:
    agent.send("ghost", task_id="t1", msg_type="request")
except NotFoundError:
    print("Agent not registered")
except AuthenticationError:
    print("Bad API key")
```

## Testing

```bash
pip install -e ".[dev]"

# Unit tests (mocked HTTP)
pytest tests/test_client.py -v

# Integration tests (requires running stream0 server)
STREAM0_URL=http://localhost:8080 pytest tests/test_integration.py -v
```
