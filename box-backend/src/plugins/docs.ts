import fp from 'fastify-plugin'
import swagger from '@fastify/swagger'
import scalarApiReference from '@scalar/fastify-api-reference'
import type { FastifyPluginAsyncTypebox } from '@fastify/type-provider-typebox'
import { APP_NAME, APP_VERSION } from '../config/meta'

const docsPlugin: FastifyPluginAsyncTypebox = async (app) => {
  if (!app.env.ENABLE_SWAGGER) {
    return
  }

  await app.register(swagger, {
    openapi: {
      info: {
        title: `${APP_NAME} API`,
        version: APP_VERSION
      },
      components: {
        securitySchemes: {
          bearerAuth: {
            type: 'http',
            scheme: 'bearer',
            bearerFormat: 'JWT'
          }
        }
      }
    }
  })

  await app.register(scalarApiReference, {
    routePrefix: '/docs'
  })
}

export default fp(docsPlugin, {
  name: 'docs'
})
