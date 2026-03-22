# Boxhouse

An agent platform for deploying and managing specialized AI workers.

Users say one thing, a group of specialized agents do their jobs, and results come back.

## Quick Start

### Build

```bash
cargo build --release
# Binary at: target/release/bh
```

### Single Machine Setup

```bash
# 1. Start server (prints admin key on first start)
bh server
#   Admin key: bh_abc123...

# 2. Connect with admin key (from another terminal)
bh login http://localhost:8080 --key bh_abc123...

# 3. Create a group and invite yourself
bh group create my-team
bh group invite my-team --description "me"
#   Key: bh_def456...

# 4. Login with group key
bh login http://localhost:8080 --key bh_def456...

# 5. Add a worker and delegate
bh worker add reviewer --instructions "Senior code reviewer. Focus on correctness."
bh delegate reviewer "Review the file src/main.rs for correctness."
bh wait
```

### Multi-Machine Setup

```bash
# Machine A: start server
bh server --host 0.0.0.0 --port 8080

# Machine A: create group and keys
bh login http://localhost:8080 --key <admin-key>
bh group create dev-team
bh group invite dev-team --description "node-b"

# Machine B: join as a worker node
bh node join http://machine-a:8080 --name gpu-box --key <group-key>

# Machine A: add worker on the remote node
bh login http://localhost:8080 --key <group-key>
bh worker add ml-agent --instructions "ML specialist." --node gpu-box
bh delegate ml-agent "Analyze this dataset."
bh wait
```

## CLI Reference

### Connection

```
bh login <server-url> --key <api-key>   Connect to server
bh logout                                Disconnect
bh skill install claude-code             Install skill for Claude Code
bh skill install codex                   Install skill for Codex
bh skill uninstall <agent>               Remove skill
bh skill show                            Print skill content to stdout
bh status                                Show connection, workers, pending tasks
```

### Server

```
bh server [--host 127.0.0.1] [--port 8080] [--db ./bh.db]
```

On first start, generates and prints an admin key.

### Workers

```
bh worker add <name> --instructions "..." [--node <node>]
bh worker ls
bh worker info <name>
bh worker update <name> --instructions "..."
bh worker stop <name>
bh worker start <name>
bh worker logs <name>
bh worker remove <name>
bh worker temp "<task>" [--instructions "..."]
```

### Delegation

```
bh delegate <worker> "<task>"       Send task (non-blocking), prints thread-id
bh delegate <worker>                Read task from stdin
bh worker temp "<task>"             One-off task (non-blocking), auto-cleanup
bh wait                             Block until all pending tasks complete
bh reply <thread-id> "<message>"    Reply to a worker's question
```

### Nodes

```
bh node join <server-url> [--name <name>] [--key <api-key>]
bh node ls
```

### Groups & Keys

```
bh group create <name>                          Create a group (admin only)
bh group ls                                     List groups (admin only)
bh group invite <group> [--description "..."]   Generate group key (admin only)
bh group keys                                   List API keys
bh group revoke <key-prefix>                    Revoke a key (admin only)
```

## Authentication

Server generates an admin key on first start. All endpoints require authentication.

- **Admin key** — server-level. Can create groups, invite members, manage nodes.
- **Group key** — scoped to one group. Can manage workers, delegate tasks, see only own group's resources.

Groups are fully isolated: workers, agents, and messages in one group are invisible to other groups.

## Architecture

```
Server
├── Admin key (server-level)
├── Group: frontend
│   ├── key: bh_abc... (alice)
│   ├── reviewer worker (local node)
│   └── doc-writer worker (local node)
├── Group: ml-team
│   ├── key: bh_def... (bob)
│   └── ml-agent worker (gpu-box node)
│
├── Node: local (auto-registered)
├── Node: gpu-box (via bh node join)
└── Node: cpu-1 (via bh node join)
```

Workers are ephemeral per-task. When a request arrives, the node daemon spawns a Claude Code CLI subprocess, runs the task, and sends the result back.

## How Workers Execute Tasks

Workers invoke `claude --print --output-format json --system-prompt "<instructions>"` with the task piped via stdin. They use the machine's existing authentication (OAuth or API key).

Multi-turn is supported: if a lead sends a `bh reply`, the daemon resumes the Claude session with `--resume <session_id>`.

## Configuration

Server config via TOML file (`--config path`) or environment variables:
- `BH_HOST` — server bind address
- `BH_PORT` — server port
- `BH_DB_PATH` — SQLite database file path
- `BH_LOG_LEVEL` — log level (info, debug, etc.)

CLI config stored at `~/.bh/config.toml`:
- `server_url` — server address (overridable via `BH_SERVER_URL`)
- `lead_id` — auto-generated stable identity
- `api_key` — stored by `bh login --key`

## License

Private. Copyright RisingWave Labs.
