import envSchema from 'env-schema'
import { Type, type Static } from '@sinclair/typebox'

const NodeEnvSchema = Type.Union([
  Type.Literal('development'),
  Type.Literal('test'),
  Type.Literal('production')
])

const LogLevelSchema = Type.Union([
  Type.Literal('fatal'),
  Type.Literal('error'),
  Type.Literal('warn'),
  Type.Literal('info'),
  Type.Literal('debug'),
  Type.Literal('trace'),
  Type.Literal('silent')
])

export const appEnvSchema = Type.Object(
  {
    NODE_ENV: Type.Optional(NodeEnvSchema),
    HOST: Type.Optional(Type.String({ minLength: 1 })),
    PORT: Type.Optional(Type.Integer({ minimum: 0, maximum: 65535 })),
    LOG_LEVEL: Type.Optional(LogLevelSchema),
    CORS_ORIGIN: Type.Optional(Type.String({ minLength: 1 })),
    ENABLE_SWAGGER: Type.Optional(Type.Boolean())
  },
  {
    additionalProperties: false
  }
)

type RawAppEnv = Static<typeof appEnvSchema>

export type AppEnv = Required<RawAppEnv>

const defaults: AppEnv = {
  NODE_ENV: 'development',
  HOST: '0.0.0.0',
  PORT: 3000,
  LOG_LEVEL: 'info',
  CORS_ORIGIN: '*',
  ENABLE_SWAGGER: true
}

export function loadEnv(): AppEnv {
  return {
    ...defaults,
    ...envSchema<RawAppEnv>({
      data: process.env,
      schema: appEnvSchema
    })
  }
}

export function buildEnv(overrides: Partial<AppEnv> = {}): AppEnv {
  return {
    ...defaults,
    ...overrides
  }
}
