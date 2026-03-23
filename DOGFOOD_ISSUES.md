# Box0 Dogfood Issues

Found during dogfood testing on 2026-03-22.

## 1. Silent failures in `b0 wait`

`b0 wait` silently swallows errors with `Err(_) => continue` (main.rs:815-817). When inbox poll returns 404 or network error, user sees "queued" forever with no error message. Also `let _ =` on ack, save_pending, and temp agent cleanup.

## 2. pending.json orphaned state

Server restart or agent deletion leaves stale entries in `~/.b0/pending.json`. `b0 wait` then tries to poll for dead agents forever. No cleanup logic, no validation, no expiry.

## 3. No database schema migration

`CREATE TABLE IF NOT EXISTS` means old databases never get new columns (e.g. `temp`). New binary + old DB causes "no such column" errors. Need `PRAGMA table_info()` + `ALTER TABLE` migration.

## 4. Agent sandbox vs workdir

Agents are sandboxed to `~/.b0/agents/{name}/` and can't access the user's codebase. When asked to "read the source code at /path/to/repo", they can't. Need `--workdir` option on `b0 agent add`.

## 5. Empty content from daemon

daemon sometimes returns empty string as task result (done message with `content=""`). `parse_claude_json` (daemon.rs:442-456) uses `.unwrap_or("(no result)")` which masks missing fields. Also no distinction between "completed with empty output" and "output capture failed".

## 6. No runtime observability

`b0 wait` only shows `"running (Ns)..."` with no visibility into what the agent is doing. No streaming, no intermediate progress messages. Need `b0 agent logs --follow` (tail -f style) and richer status in `b0 wait`.
