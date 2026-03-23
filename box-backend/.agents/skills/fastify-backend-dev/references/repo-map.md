# Repo map for box0-flow Fastify backend

Use this as the source of truth for where changes belong.

## Files and responsibilities

### App bootstrap
- `./src/app.ts`
  - builds the Fastify instance
  - applies `TypeBoxTypeProvider`
  - decorates `app.env`
  - registers shared plugins and module hub
  - defines not-found and error handlers

- `./src/server.ts`
  - loads env
  - starts listening
  - handles SIGINT/SIGTERM shutdown

### Config
- `./src/config/env.ts`
  - env schema and defaults
  - exports `loadEnv()` and `buildEnv()`
  - all env additions should be made here

- `./src/config/logger.ts`
  - logger setup for test/development/production
  - redaction rules belong here

- `./src/config/meta.ts`
  - app/package metadata used by routes and docs

### Shared plugins
- `./src/plugins/security.ts`
  - helmet and CORS

- `./src/plugins/docs.ts`
  - swagger and swagger-ui
  - controlled by `ENABLE_SWAGGER`

### Feature modules
- `./src/modules/index.ts`
  - central registration hub for all backend modules

- `./src/modules/system/routes.ts`
  - `/` metadata route

- `./src/modules/health/routes.ts`
  - `/health` route

### Tests
- `./test/app.test.ts`
  - integration-style tests using `app.inject()`

### Runtime files
- `./.env.example`
  - sample env values; keep in sync with `src/config/env.ts`

- `./Dockerfile`
  - multi-stage pnpm build and runtime image

## Placement rules

### Add a new endpoint
- Usually create or update `src/modules/<feature>/routes.ts`
- Register it in `src/modules/index.ts`
- Add/update tests under `test/`

### Add shared request behavior
- Put it in `src/plugins/` if it applies across modules

### Add env/config
- Put schema/defaults in `src/config/env.ts`
- Update `.env.example`

### Change logging/error shape
- Update shared behavior in `src/config/logger.ts` or `src/app.ts`
- Do not patch individual routes unless the task specifically needs route-specific behavior

## Existing behavior to preserve by default
- routes are feature-oriented under `src/modules`
- app-wide error responses contain `error`, `message`, and `requestId`
- env defaults produce a fully usable local/test app
- docs are optional via `ENABLE_SWAGGER`
