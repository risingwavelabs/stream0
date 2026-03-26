# Multi-machine setup

Distribute agents across multiple machines. Each machine uses its own local credentials. No secrets are forwarded.

## Topology

```
                    ┌──────────────────────────┐
                    │      Box0 Server         │
                    │       Machine A          │
                    │  ┌──────────────────────┐│
                    │  │  inbox / routing     ││
                    │  └──────────────────────┘│
                    └────────────┬─────────────┘
                                 │  HTTP
              ┌──────────────────┼──────────────────┐
              │                  │                  │
    ┌─────────▼────────┐ ┌───────▼──────────┐ ┌────▼─────────────┐
    │   Machine A      │ │   Machine B      │ │   Machine C      │
    │   (local)        │ │   (gpu-box)      │ │   (cloud)        │
    │                  │ │                  │ │                  │
    │ ┌──────────────┐ │ │ ┌──────────────┐ │ │ ┌──────────────┐ │
    │ │  ux-expert   │ │ │ │  ml-agent    │ │ │ │  reviewer    │ │
    │ │  architect   │ │ │ │  (GPU tasks) │ │ │ │  (cloud cred)│ │
    │ └──────────────┘ │ │ └──────────────┘ │ │ └──────────────┘ │
    │  own credentials │ │  own credentials │ │  own credentials │
    └──────────────────┘ └──────────────────┘ └──────────────────┘
```

## Setup

### 1. Start the server with external access

The server must bind to `0.0.0.0` for remote machines to connect:

```bash
b0 server --host 0.0.0.0
```

### 2. Join a remote machine

On the remote machine, join the server:

```bash
b0 machine join http://server-ip:8080 --name gpu-box --key <key>
```

The daemon starts polling the server for tasks.

### 3. Assign agents to the machine

Back on the server machine:

```bash
b0 agent add ml-agent --instructions "ML specialist." --machine gpu-box
```

### 4. Delegate tasks

```bash
b0 delegate ml-agent "Analyze this dataset."
```

```bash
b0 wait
```

The task is routed to the remote machine. Claude Code or Codex runs there using that machine's local credentials and compute.

## How it works

- Remote machines use long-polling (`/machines/{id}/poll`) with up to 30s timeout for efficient task pickup.
- Each machine runs its own daemon that spawns Claude Code or Codex locally.
- Agents use the machine's existing authentication (OAuth or API key). No credential forwarding.
- Only the machine owner can deploy agents to their machine.
- The server handles routing: tasks go to whichever machine owns the target agent.

## List machines

```bash
b0 machine ls
```

Shows all connected machines and their status.
