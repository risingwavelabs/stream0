import Fastify from 'fastify'
import sensible from '@fastify/sensible'
import {
  TypeBoxTypeProvider,
  TypeBoxValidatorCompiler
} from '@fastify/type-provider-typebox'
import { loadEnv, type AppEnv } from './config/env'
import { buildLoggerConfig } from './config/logger'
import modulesPlugin from './modules'
import docsPlugin from './plugins/docs'
import securityPlugin from './plugins/security'
import supabasePlugin from './plugins/supabase'

export type BuildAppOptions = {
  env?: AppEnv
}

type ErrorResponse = {
  error: string
  message: string
  requestId: string
}

type FastifyErrorLike = Error & {
  statusCode?: number
  validation?: unknown
}

function isValidationError(error: unknown): error is FastifyErrorLike {
  return typeof error === 'object' && error !== null && 'validation' in error
}

function toFastifyError(error: unknown): FastifyErrorLike {
  if (error instanceof Error) {
    return error
  }

  return new Error('Unknown error')
}

export function buildApp(options: BuildAppOptions = {}) {
  const env = options.env ?? loadEnv()
  const app = Fastify({
    logger: buildLoggerConfig(env)
  }).withTypeProvider<TypeBoxTypeProvider>()

  app.decorate('env', env)
  app.setValidatorCompiler(TypeBoxValidatorCompiler)

  app.register(sensible)
  app.register(securityPlugin)
  app.register(docsPlugin)
  app.register(supabasePlugin)
  app.register(modulesPlugin)

  app.setNotFoundHandler((request, reply) => {
    return reply.status(404).send({
      error: 'Not Found',
      message: `Route ${request.method} ${request.url} not found`,
      requestId: request.id
    } satisfies ErrorResponse)
  })

  app.setErrorHandler((error, request, reply) => {
    if (isValidationError(error)) {
      return reply.status(400).send({
        error: 'Bad Request',
        message: error.message,
        requestId: request.id
      } satisfies ErrorResponse)
    }

    const appError = toFastifyError(error)
    const statusCode =
      typeof appError.statusCode === 'number' && appError.statusCode >= 400
        ? appError.statusCode
        : 500

    if (statusCode >= 500) {
      request.log.error({ err: appError }, 'request failed')
    }

    return reply.status(statusCode).send({
      error: statusCode >= 500 ? 'Internal Server Error' : appError.name,
      message: appError.message,
      requestId: request.id
    } satisfies ErrorResponse)
  })

  return app
}
