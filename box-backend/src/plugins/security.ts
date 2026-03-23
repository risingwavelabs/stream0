import fp from 'fastify-plugin'
import cors from '@fastify/cors'
import helmet from '@fastify/helmet'
import type { FastifyPluginAsyncTypebox } from '@fastify/type-provider-typebox'

const securityPlugin: FastifyPluginAsyncTypebox = async (app) => {
  const { CORS_ORIGIN } = app.env

  await app.register(helmet, {
    global: true
  })

  await app.register(cors, {
    origin: CORS_ORIGIN === '*' ? true : CORS_ORIGIN
  })
}

export default fp(securityPlugin, {
  name: 'security'
})
