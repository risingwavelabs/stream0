# Cron jobs

Schedule recurring tasks. Box0 runs them automatically on your configured interval.

## Quick start

```bash
b0 cron add --every 6h "Check production logs for errors and summarize."
```

This creates a temporary agent and runs the task every 6 hours.

## Intervals

Supported intervals: `30s`, `5m`, `1h`, `6h`, `1d`.

## Use a specific agent

By default, `b0 cron add` creates a temporary agent. To use an existing agent instead:

```bash
b0 agent add prod-monitor --instructions "You monitor production systems. Be concise."
b0 cron add --agent prod-monitor --every 1h "Check for error spikes in the last hour."
```

## End date

Stop a cron job after a specific date:

```bash
b0 cron add --every 1d --until 2026-04-01 "Generate the daily standup summary."
```

Accepts dates like `2026-04-01` or full timestamps like `2026-04-01T12:00:00Z`.

## Notifications

Get results via webhook or Slack:

```bash
b0 cron add --every 6h --webhook https://example.com/hook "Check uptime."
b0 cron add --every 1h --slack "#ci-alerts" "Run the test suite and report failures."
```

See [Slack setup](slack.md) for configuring Slack notifications.

## Manage cron jobs

List all scheduled tasks:

```bash
b0 cron ls
```

Temporarily disable a job without deleting it:

```bash
b0 cron disable <id>
```

Re-enable it:

```bash
b0 cron enable <id>
```

Remove a job (also cleans up any auto-created temporary agent):

```bash
b0 cron remove <id>
```
