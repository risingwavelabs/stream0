import { Type } from '@sinclair/typebox'
import type { FastifyPluginAsyncTypebox } from '@fastify/type-provider-typebox'

import { APP_NAME, APP_VERSION } from '../../config/meta'

const systemRoutes: FastifyPluginAsyncTypebox = async (app) => {
  app.get(
    '/',
    {
      schema: {
        tags: ['system'],
        response: {
          200: Type.Object({
            name: Type.String(),
            version: Type.String(),
            environment: Type.Union([
              Type.Literal('development'),
              Type.Literal('test'),
              Type.Literal('production')
            ])
          })
        }
      }
    },
    () => ({
      name: APP_NAME,
      version: APP_VERSION,
      environment: app.env.NODE_ENV
    })
  )
}

export default systemRoutes
