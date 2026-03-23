import type { FastifyPluginAsyncTypebox } from '@fastify/type-provider-typebox'

import authRoutes from './auth/routes'
import healthRoutes from './health/routes'
import systemRoutes from './system/routes'

const modulesPlugin: FastifyPluginAsyncTypebox = async (app) => {
  await app.register(authRoutes)
  await app.register(systemRoutes)
  await app.register(healthRoutes)
}

export default modulesPlugin
