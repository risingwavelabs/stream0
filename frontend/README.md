# Box0 Frontend

TanStack Start web dashboard for Box0.

## API architecture

- Frontend calls `box-backend` (BFF).
- `box-backend` proxies/composes `box0-core`.
- Frontend should not directly call `box0-core` in the target architecture.

Current migration status:

- Auth and system routes are already in `box-backend`.
- Some dashboard reads still call legacy `box0-core` endpoints directly and should be migrated to backend BFF routes.

## OpenAPI and SDK generation

This frontend uses `@hey-api/openapi-ts` to generate typed API clients from backend OpenAPI.

Input spec:

- `../box-backend/openapi/swagger.json`

Generate client:

```bash
pnpm api:gen
```

Generated files:

- `src/lib/api-gen/*` (auto-generated, do not edit manually)

Runtime wrapper:

- `src/lib/backend-api.ts`

High-priority migration mapping:

- Workspaces: `/api/workspaces` -> core `/workspaces`
- Machines: `/api/machines` -> core `/machines`
- Users: `/api/users` -> core `/users`
- Tasks: `/api/workspaces/:workspace/tasks` -> core `/workspaces/{workspace_name}/tasks`

Workflow for API changes:

1. Update backend routes in `box-backend`.
2. Regenerate backend OpenAPI:

```bash
pnpm --dir ../box-backend swagger:generate
```

3. Regenerate frontend SDK:

```bash
pnpm api:gen
```

This mirrors the `boxcrew` workflow where frontend API types are generated from backend OpenAPI contracts.

## Local development

```bash
pnpm install
pnpm dev
```

Build:

```bash
pnpm build
```
