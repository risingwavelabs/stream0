# stream0

HTTP-native event streaming for AI agents. Built in Go for performance and single-binary deployment.

## Features

- **Single Binary**: One executable, zero dependencies
- **Fast**: 10x faster than Python prototype
- **Consumer Groups**: Built-in load balancing with visibility timeouts
- **SQLite Backend**: Zero external dependencies, WAL mode for concurrency
- **HTTP API**: Simple REST + WebSocket

## Quick Start

```bash
# Build and run
go build -o stream0 .
./stream0

# Server runs on http://127.0.0.1:8080
```

## API Usage

### Create Topic
```bash
curl -X POST http://localhost:8080/topics \
  -H "Content-Type: application/json" \
  -d '{"name": "tasks", "retention_days": 7}'
```

### Produce Message
```bash
curl -X POST http://localhost:8080/topics/tasks/messages \
  -H "Content-Type: application/json" \
  -d '{"payload": {"action": "analyze"}}'
```

### Consume Messages
```bash
curl "http://localhost:8080/topics/tasks/messages?group=workers&max=10&timeout=5"
```

### Acknowledge
```bash
curl -X POST http://localhost:8080/messages/{id}/ack \
  -H "Content-Type: application/json" \
  -d '{"group": "workers"}'
```

## Testing

```bash
python3 test_go.py
```

## Configuration

Environment variables:
- `STREAM0_SERVER_HOST` - Bind address (default: 127.0.0.1)
- `STREAM0_SERVER_PORT` - Port (default: 8080)
- `STREAM0_DB_PATH` - Database path (default: ./stream0.db)

## Architecture

```
┌──────────┐     ┌──────────┐     ┌──────────┐
│ Agent A  │────→│ stream0 │────→│ Agent B  │
│ Producer │     │   (Go)   │     │ Consumer │
└──────────┘     └────┬─────┘     └──────────┘
                      │
                 ┌────┴────┐
                 │ SQLite  │
                 │  (WAL)  │
                 └─────────┘
```

## Performance

- Single binary: ~20MB
- Startup time: <100ms
- Throughput: 10K+ messages/sec
- Memory: ~10MB baseline

## License

MIT
