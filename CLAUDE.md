# CLAUDE.md

This file is for Claude Code (or any AI agent) working on Stream0. Read this first.

## What is Stream0?

An agent communication layer. Every agent gets an inbox. Agents send messages to each other's inboxes, grouped by `task_id`. Supports multi-turn conversations (request → question → answer → done).

## Project structure

```
├── main.go           # Entry point, graceful shutdown
├── server.go         # HTTP handlers (v1 topic + v2 inbox)
├── database.go       # SQLite operations, schema
├── config.go         # YAML config + env var loading
├── server_test.go    # HTTP endpoint tests
├── database_test.go  # Database layer tests
├── go.mod / go.sum   # Go dependencies
├── sdk/python/       # Python SDK
│   ├── stream0/      # Package (client.py, exceptions.py)
│   └── tests/        # Unit + integration tests
└── docs/             # PRD, architecture docs
```

## Build and test

```bash
# Build
go build -o stream0 .

# Run
./stream0                          # default config
./stream0 -config stream0.yaml    # custom config

# Test (Go)
go test -v ./...

# Test (Python SDK)
cd sdk/python && pip install -e ".[dev]" && pytest tests/test_client.py -v

# Integration tests (needs running server)
STREAM0_URL=http://localhost:8080 pytest tests/test_integration.py -v
```

## Key APIs

### v2 Inbox Model (primary)

- `POST /agents` — register agent `{"id": "agent-name"}`
- `POST /agents/{id}/inbox` — send message `{"task_id", "from", "type", "content"}`
- `GET /agents/{id}/inbox?status=unread&task_id=X&timeout=10` — poll inbox
- `POST /inbox/messages/{id}/ack` — mark as read
- `GET /tasks/{task_id}/messages` — conversation history

Message types: `request`, `question`, `answer`, `done`, `failed`

### v1 Topic Model (still works, backward compatible)

- `POST /topics` — create topic
- `POST /topics/{name}/messages` — publish
- `GET /topics/{name}/messages?group=X&timeout=5` — consume (long-polling)
- `POST /messages/{id}/ack` — acknowledge

## Important technical details

- **SQLite library**: Use `github.com/mattn/go-sqlite3` (CGO). Do NOT use `modernc.org/sqlite` — it crashes with OOM on EC2 t3.micro.
- **Config loading**: No envconfig library. Manual `os.Getenv()` only overrides when set. See config.go.
- **Auth**: API key via `X-API-Key` header. Keys in YAML config under `auth.api_keys`. Constant-time comparison.
- **CGO required**: `mattn/go-sqlite3` requires CGO. Cannot cross-compile from macOS to Linux. Build on the target machine.
- **Long-polling**: Both v1 consume and v2 inbox support long-polling with `timeout` param.

## Deployment

- **EC2**: Build on instance with `CGO_ENABLED=1 go build -o stream0 .`, systemd service at `/etc/systemd/system/stream0.service`
- **Config**: `/etc/stream0/stream0.yaml`
- **Data**: `/var/lib/stream0/stream0.db`
- **API keys**: Only in config file on server, never in code or chat

## Common tasks

### Add a new inbox endpoint

1. Add handler in `server.go` (pattern: `func (s *Server) myHandler(c *gin.Context)`)
2. Register route in `setupRoutes()` under the v2 section
3. Add database method in `database.go` if needed
4. Add tests in both `server_test.go` and `database_test.go`
5. Update Python SDK in `sdk/python/stream0/client.py`
6. Add Python tests in `sdk/python/tests/test_client.py`

### Deploy an update

```bash
# Upload source to EC2
scp *.go go.mod go.sum ubuntu@<IP>:/tmp/stream0-build/

# SSH in and build
ssh ubuntu@<IP>
export PATH=/usr/local/go/bin:$PATH
cd /tmp/stream0-build && CGO_ENABLED=1 go build -o stream0 .

# Deploy
sudo systemctl stop stream0
sudo cp stream0 /usr/local/bin/stream0
sudo systemctl start stream0
```

## Do not

- Do not use `modernc.org/sqlite` — it fails on EC2
- Do not use `kelseyhightower/envconfig` — it overwrites YAML values with defaults
- Do not commit API keys or secrets
- Do not cross-compile with `CGO_ENABLED=0` — the binary will crash at runtime
