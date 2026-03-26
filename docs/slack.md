# Slack notifications

Box0 can send Slack messages when agents complete or fail a task.

## Setup

### 1. Create a Slack app from manifest

Go to [api.slack.com/apps](https://api.slack.com/apps), click **Create New App**, then choose **From a manifest**.

Select your workspace and paste this JSON:

```json
{
  "_metadata": {
    "major_version": 1,
    "minor_version": 1
  },
  "display_information": {
    "name": "Box0",
    "description": "Box0 agent notifications"
  },
  "features": {
    "bot_user": {
      "display_name": "Box0",
      "always_online": false
    }
  },
  "oauth_config": {
    "scopes": {
      "bot": [
        "chat:write"
      ]
    }
  },
  "settings": {
    "org_deploy_enabled": false,
    "socket_mode_enabled": false,
    "is_hosted": false
  }
}
```

Click **Create**, then **Install to Workspace** and **Allow**.

### 2. Copy the bot token

Go to **OAuth & Permissions** in the left sidebar and copy the **Bot User OAuth Token** (starts with `xoxb-`).

### 3. Invite the bot

In Slack, invite the bot to any channel you want notifications in:

```
/invite @Box0
```

### 4. Configure Box0

Set the token as an environment variable before starting the server:

```bash
export B0_SLACK_TOKEN=xoxb-your-token-here
b0 server
```

## Usage

Specify a Slack channel when creating an agent:

```bash
b0 agent add monitor --instructions "Watch for regressions." --slack "#ops"
```

Or with a cron job:

```bash
b0 cron add --every 1h "Check production health." --slack "#ci-alerts"
```

When the agent finishes (or fails), Box0 posts a message to the channel:

```
[Box0] monitor done: No regressions found in the latest deployment.
```

```
[Box0] monitor failed: Connection timeout reaching production API.
```

## Notes

- The message includes the agent name, status (done/failed), and result text (truncated to 500 characters).
- Both `--slack` and `--webhook` can be used on the same agent.
- The server must have `B0_SLACK_TOKEN` set. If the token is missing, Slack notifications are silently skipped.
