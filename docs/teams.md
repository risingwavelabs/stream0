# Workspaces

Multiple people can share a Box0 server. Each person gets their own API key and can be added to workspaces for shared access.

## Create a user

On the server machine (admin):

```bash
b0 invite alice
```

This prints Alice's API key.

## Create a shared workspace

```bash
b0 workspace create dev-team
```

```bash
b0 workspace add-member dev-team <alice-user-id>
```

## Connect from another machine

On Alice's laptop:

```bash
b0 login http://server:8080 --key <alice-key>
```

The CLI auto-configures the default workspace from Alice's membership.

## Work within a workspace

```bash
b0 agent add reviewer --workspace dev-team --instructions "Code reviewer."
```

```bash
b0 delegate --workspace dev-team reviewer "Review src/main.rs"
```

```bash
b0 wait
```

## How workspaces work

- Each user gets a personal workspace on creation.
- Users can be in multiple workspaces. Use `--workspace` to select which one.
- Agents in a workspace are visible to all workspace members.
- Only the agent creator can remove or update their agents.

## List workspaces

```bash
b0 workspace ls
```
