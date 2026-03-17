# Self-Hosting Guide

## Quick Start

```bash
# Build
cargo build --release

# Run with defaults (localhost:8080, SQLite in current dir)
./target/release/stream0

# Run with config
./target/release/stream0 --config stream0.yaml
```

## Configuration

### Config file

Create `stream0.yaml`:

```yaml
server:
  host: 0.0.0.0
  port: 8080

database:
  path: /var/lib/stream0/stream0.db

log:
  level: info
  format: json

auth:
  api_keys:
    - your-secret-key-here
```

### Environment variables

Override any config value:

| Variable | Description | Default |
|----------|-------------|---------|
| `STREAM0_SERVER_HOST` | Bind address | `127.0.0.1` |
| `STREAM0_SERVER_PORT` | Port | `8080` |
| `STREAM0_DB_PATH` | Database path | `./stream0.db` |
| `STREAM0_LOG_LEVEL` | Log level | `info` |
| `STREAM0_LOG_FORMAT` | `json` or `text` | `json` |
| `STREAM0_API_KEY` | Add an API key | (none) |

## Production Deployment (systemd)

### 1. Create user and directories

```bash
sudo useradd -r -s /bin/false stream0
sudo mkdir -p /etc/stream0 /var/lib/stream0
sudo chown stream0:stream0 /var/lib/stream0
```

### 2. Install binary

```bash
# Build on the target machine (or cross-compile)
cargo build --release
sudo cp target/release/stream0 /usr/local/bin/
```

### 3. Add config

```bash
sudo tee /etc/stream0/stream0.yaml << 'EOF'
server:
  host: 0.0.0.0
  port: 8080
database:
  path: /var/lib/stream0/stream0.db
log:
  level: info
  format: json
auth:
  api_keys:
    - GENERATE_A_RANDOM_KEY_HERE
EOF
sudo chmod 600 /etc/stream0/stream0.yaml
```

### 4. Create systemd service

```bash
sudo tee /etc/systemd/system/stream0.service << 'EOF'
[Unit]
Description=stream0 message bus
After=network.target

[Service]
Type=simple
User=stream0
Group=stream0
ExecStart=/usr/local/bin/stream0 -config /etc/stream0/stream0.yaml
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF
```

### 5. Start

```bash
sudo systemctl daemon-reload
sudo systemctl enable stream0
sudo systemctl start stream0
```

### 6. Verify

```bash
curl http://localhost:8080/health
# {"status":"healthy","version":"0.1.0-go"}
```

## Authentication

When `api_keys` is set in config, all endpoints except `/health` require:

```bash
curl -H "X-API-Key: your-secret-key" http://localhost:8080/agents
```

## Backup

SQLite WAL mode allows live backups:

```bash
sqlite3 /var/lib/stream0/stream0.db ".backup /backup/stream0.db"
```

## Monitoring

- Health: `GET /health`
- Logs: `journalctl -u stream0 -f`

## Important notes

- **Rust required**: Install with `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **SQLite bundled**: The `rusqlite` crate compiles SQLite from source. No system SQLite dependency needed.
- **Swap for small instances**: EC2 t3.micro may need 2GB swap for builds: `sudo fallocate -l 2G /swapfile && sudo mkswap /swapfile && sudo swapon /swapfile`
