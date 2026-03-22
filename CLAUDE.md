# CLAUDE.md

## Project

Box0 is a multi-agent platform. It lets you run multiple AI agents in parallel across machines. Rust codebase, single binary (`b0`). npm package: `@box0/cli`.

## Conventions

- Never use em dashes. Use periods, commas, colons, or "- " instead.
- All content in English.
- README code blocks must be copy-paste safe. No inline comments on the same line as commands. No blocking commands (like `b0 server`) in the same block as other commands. Use blockquotes for conversation examples, not code blocks.
- Always test changes before committing. Run `cargo test` at minimum (unit + integration tests). For user-facing features, run `tests/e2e.sh` which requires Claude Code or Codex.
- After code changes, always update README.md, CLAUDE.md, docs/, and the skill content in config.rs if affected. This is critical.
- Commit messages: imperative mood, concise first line, details in body.
- No documentation files unless explicitly requested.

## Architecture

- `src/lib.rs` - Library crate, re-exports all modules
- `src/main.rs` - CLI entry point, all subcommand dispatch
- `src/server.rs` - Axum HTTP server, route handlers, auth middleware, `build_router()` for tests
- `src/db.rs` - SQLite schema, models, all queries
- `src/daemon.rs` - Node daemon (local + remote), polls worker inboxes, spawns runtime
- `src/client.rs` - HTTP client for CLI-to-server communication
- `src/config.rs` - Server config, CLI config, skill installation, pending state

## Auth model

- Users have unique keys. Keys identify users, not groups.
- Each user gets a personal group on creation.
- Users can be in multiple groups. `--group` flag selects which group to operate in. Defaults to `default_group` in config.
- `b0 login` auto-sets `default_group` from user's first group. No need for manual config.
- Nodes are owned by users. Only the owner can deploy workers to their node.
- Workers track `registered_by`. Only the creator can remove/update/stop their workers.
- Admin user is created on first server start. Server auto-writes CLI config (no login needed on server machine).

## Worker execution

- Each worker has its own isolated directory under `~/.b0/workers/<name>/`.
- Workers support multiple runtimes: `auto` (default), `claude`, or `codex`.
  - `auto` prefers Claude Code if installed, falls back to Codex.
  - Set per-worker via `--runtime claude` or `--runtime codex`.
- Daemon spawns the runtime CLI in the worker's directory.
  - Claude: `claude --print --output-format json --system-prompt "<instructions>"`, task piped via stdin.
  - Codex: `codex exec --json --full-auto --skip-git-repo-check [-C <dir>] "<instructions>\n\n<task>"`, task as argument.
  - Codex output is JSONL. Parse `item.completed` events, extract `item.text`.
  - Codex requires `--skip-git-repo-check` because worker directories are not git repos.
- Session IDs are tracked per thread for multi-turn conversations (`--resume`, Claude only). Codex does not support session resume.
- Multi-turn: `b0 delegate --thread <id>` sends "answer" message, daemon resumes Claude session.
- Windows compatibility: runtime detection uses `where` instead of `which`.

## Multi-machine

- Server must bind to `0.0.0.0` for remote access (not the default `127.0.0.1`).
- Remote nodes join via `b0 node join <url> --name <name> --key <key>`.
- Remote daemon fetches worker list via `GET /nodes/{id}/workers` (cross-group endpoint).
- Each node runs its own daemon that polls for tasks and spawns the local runtime.
- Workers use the machine's local Claude/Codex authentication. No credential forwarding.

## CLI design

- `--group` is optional when `default_group` is set in config.
- `b0 server` on first start auto-configures `~/.b0/config.toml` (server_url, api_key, default_group).
- `b0 login` on remote machines auto-sets default_group from user's first group.
- `b0 worker temp` is non-blocking (same as `b0 delegate`). Temp workers auto-cleanup on `b0 wait`.
- `b0 delegate` without `--thread` creates new conversation. With `--thread` continues existing one.
- `b0 skill install claude-code` writes `~/.claude/skills/b0/SKILL.md` (directory format, not plain file).
- `b0 skill install codex` appends marked section to `~/.codex/AGENTS.md`.

## Distribution

- npm package: `@box0/cli`
- Install: `npm install -g @box0/cli@latest`
- CI builds 5 platforms on tag push: darwin-arm64, darwin-x64, linux-x64, linux-arm64, windows-x64
- npm version auto-synced from git tag in CI
- `install.js` downloads binary from GitHub releases matching package.json version
- Release flow: `git tag v0.x.0 && git push origin v0.x.0`

## DB schema

Tables: users, groups, group_members, agents, inbox_messages, nodes, workers. Group name is used as tenant for isolation.

## Testing

- Unit tests in `src/db.rs` (8 tests covering users, groups, workers, inbox, nodes, ownership).
- API integration tests in `tests/api.rs` (10 tests). Start a real Axum server per test with temp DB, test via HTTP client. No Claude/Codex needed. Run with `cargo test`.
- E2e script in `tests/e2e.sh`. Requires Claude Code or Codex installed. Starts real server, runs CLI commands, verifies results. Run manually before releases.
- CI runs `cargo test` on every push/PR via `.github/workflows/ci.yml`.
- `b0 reset` deletes DB, config, and skills for clean slate.
