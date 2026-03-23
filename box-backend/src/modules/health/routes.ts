import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsyncTypebox } from '@fastify/type-provider-typebox'

const healthRoutes: FastifyPluginAsyncTypebox = async (app) => {
  app.get(
    '/health',
    {
      schema: {
        tags: ['system'],
        response: {
          200: Type.Object({
            status: Type.Literal('ok')
          })
        }
      }
    },
    () => ({ status: 'ok' as const })
  )
}

export default healthRoutes
