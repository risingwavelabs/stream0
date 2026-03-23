# box-backend

Fastify backend for Box0 web clients.

## Role in architecture

- `frontend` should call `box-backend`.
- `box-backend` should proxy or compose `box0-core` APIs.
- `box0-core` should stay internal and not be directly called by browser clients.

This follows the same contract-first workflow used in `boxcrew`: backend emits OpenAPI, frontend consumes generated SDK/types.

## OpenAPI workflow

1. Update routes in `src/modules/**/routes.ts`.
2. Regenerate OpenAPI:

```bash
pnpm swagger:generate
```

3. Regenerate frontend SDK:

```bash
pnpm --dir ../frontend api:gen
```

Generated spec path:

- `openapi/swagger.json`

## Current public routes

- `GET /`
- `GET /health`
- `POST /auth/login/password`
- `POST /auth/login/otp/request`
- `POST /auth/login/otp/verify`
- `POST /auth/refresh`
- `GET /auth/me`
- `GET /docs` (Swagger UI, enabled when `ENABLE_SWAGGER=true`)

## Planned BFF mapping for dashboard pages

| Frontend page | Backend endpoint (target) | box0-core endpoint |
| --- | --- | --- |
| Sidebar workspace selector | `GET /api/workspaces` | `GET /workspaces` |
| Machines list/detail | `GET /api/machines`, `GET /api/machines/:machineId/agents` | `GET /machines`, `GET /machines/{machine_id}/agents` |
| Users list | `GET /api/users` | `GET /users` |
| Tasks list/detail | `GET /api/workspaces/:workspace/tasks`, `GET /api/workspaces/:workspace/tasks/:taskId` | `GET /workspaces/{workspace_name}/tasks`, `GET /workspaces/{workspace_name}/tasks/{task_id}` |
| Task chat | `POST /api/workspaces/:workspace/tasks/:taskId/messages` | `POST /workspaces/{workspace_name}/tasks/{task_id}/messages` |
| Agents list/detail | `GET /api/workspaces/:workspace/agents`, `GET /api/workspaces/:workspace/agents/:name` | `GET /workspaces/{workspace_name}/agents`, `GET /workspaces/{workspace_name}/agents/{name}` |
| Cron jobs | `GET/POST /api/workspaces/:workspace/cron`, `PUT/DELETE /api/workspaces/:workspace/cron/:cronId` | `GET/POST /workspaces/{workspace_name}/cron`, `PUT/DELETE /workspaces/{workspace_name}/cron/{cron_id}` |
