# Change checklist for Fastify backend tasks

Use this checklist before finishing backend changes in `box0-flow`.

## Before coding
- Read the target files first.
- Confirm whether the change belongs in a feature module, shared plugin, config, or app bootstrap.
- Prefer extending the existing pattern over introducing a new layer.

## During implementation
- Use TypeScript.
- Use TypeBox schemas for route contracts.
- Keep `process.env` access centralized in `src/config/env.ts`.
- Keep API behavior unchanged unless the task explicitly requests a behavior change.
- Keep plugins/module registration centralized.

## If adding a route
- Place it under `src/modules/<feature>/routes.ts`.
- Register it via `src/modules/index.ts`.
- Add response schema typing.
- Add or update an `app.inject()` test.

## If adding env/config
- Update `src/config/env.ts`.
- Update `.env.example`.
- Ensure defaults keep tests and local dev simple.

## If changing shared behavior
- Logger changes go through `src/config/logger.ts`.
- Error/not-found shape changes go through `src/app.ts`.
- Security or docs behavior goes through `src/plugins/*.ts`.

## Validation
Run what applies:
- `pnpm typecheck`
- `pnpm lint`
- `pnpm test`

## Avoid
- duplicate route registries
- duplicate env parsing
- broad refactors unrelated to the task
- changing Docker/runtime setup without task scope
- leaving tests unupdated when behavior changed
