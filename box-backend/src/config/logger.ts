import type { FastifyServerOptions } from 'fastify'

import type { AppEnv } from './env'

type LoggerConfig = Exclude<FastifyServerOptions['logger'], undefined>

export function buildLoggerConfig(env: AppEnv): LoggerConfig {
  if (env.NODE_ENV === 'test') {
    return false
  }

  const baseLogger = {
    level: env.LOG_LEVEL,
    redact: {
      paths: [
        'req.headers.authorization',
        'req.headers.cookie',
        'res.headers["set-cookie"]'
      ],
      censor: '[REDACTED]'
    }
  } satisfies Exclude<LoggerConfig, boolean>

  if (env.NODE_ENV === 'development') {
    return {
      ...baseLogger,
      transport: {
        target: 'pino-pretty',
        options: {
          translateTime: 'SYS:standard',
          ignore: 'pid,hostname'
        }
      }
    }
  }

  return baseLogger
}
