---
name: fastify-backend-dev
description: Use when working on this backend’s TypeScript/Fastify files such as `src/**/*.ts`, `test/**/*.ts`, `package.json`, `.env.example`, or `Dockerfile`, especially for adding modules or routes, changing config/plugins, updating validation/logging/error handling, or extending tests. Apply project rules: feature-first modules under `src/modules`, shared infrastructure under `src/config` and `src/plugins`, schema-first TypeBox routes, `buildApp()`/`server.ts` separation, `pnpm` + `oxlint` + `vitest` verification.
---

# Fastify Backend Dev

Apply these rules when editing the `box-backend` backend.

## Core constraints

- Keep the current stack: Fastify, TypeScript, TypeBox, `env-schema`, `pnpm`, `oxlint`, `vitest`.
- Prefer the smallest change that matches the existing architecture.
- Do not introduce DI containers, autoload magic, repository/service/controller layering, ORM scaffolding, or other abstractions unless the user explicitly asks for them.

## Project layout

- `src/app.ts`: construct Fastify, register shared plugins and modules, set validators and global handlers.
- `src/server.ts`: process bootstrap only (`loadEnv()`, `buildApp()`, `listen()`, signal handling).
- `src/config/*`: shared configuration and metadata.
- `src/plugins/*`: cross-cutting plugins such as security and docs.
- `src/modules/index.ts`: central module registration hub.
- `src/modules/<feature>/routes.ts`: feature-owned routes.
- `test/*.test.ts`: inject-based tests through `buildApp()`.

## Module rules

When adding or changing feature behavior:

1. Put new endpoints under `src/modules/<feature>/routes.ts`.
2. Register the module in `src/modules/index.ts`.
3. Keep small modules simple: one `routes.ts` file is enough.
4. Only split into extra files like `schema.ts` or `index.ts` when a module has clearly grown beyond a tiny route file.
5. Keep shared infrastructure out of modules unless it is feature-specific.

## Route conventions

- Use `FastifyPluginAsyncTypebox` for route modules.
- Define request and response schemas with TypeBox in the route file unless the module has grown enough to justify extraction.
- Keep handlers straightforward and close to the route definition.
- Preserve the current error response shape: `{ error, message, requestId }`.
- Prefer explicit plugin/module registration over dynamic discovery.

## Config, logging, and plugin conventions

- Validate env in `src/config/env.ts` with `env-schema`.
- Keep startup/runtime separation: `loadEnv()` in `server.ts`, `buildApp()` in `app.ts`.
- Reuse `buildLoggerConfig()` for logger behavior instead of inlining logger setup elsewhere.
- Keep Swagger behind `ENABLE_SWAGGER`.
- Keep security concerns in `src/plugins/security.ts`.

## Testing conventions

- Prefer `app.inject()` tests over booting a real server.
- Build test apps through `buildApp({ env: buildEnv(...) })`.
- Keep tests focused on behavior: route success, route metadata, and error shape.
- When a change affects API behavior, update or add tests in `test/*.test.ts`.

## Verification

After non-trivial backend changes, run:

```bash
pnpm typecheck
pnpm lint
pnpm test
pnpm build
```

If you change routes or startup behavior, also smoke-check `pnpm dev` when appropriate.

## Avoid

- Importing from sibling projects.
- Creating abstractions for one-off logic.
- Moving app-level concerns into modules.
- Mixing process startup logic into `app.ts`.
- Skipping validation/tests after structural backend changes.

## References

- [Repo map](./references/repo-map.md): detailed file breakdown and placement rules.
- [Change checklist](./references/change-checklist.md): pre-merge checklist for common backend changes.