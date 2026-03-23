import type { FastifyPluginAsyncTypebox } from '@fastify/type-provider-typebox'

import healthRoutes from './health/routes'
import systemRoutes from './system/routes'

const modulesPlugin: FastifyPluginAsyncTypebox = async (app) => {
  await app.register(systemRoutes)
  await app.register(healthRoutes)
}

export default modulesPlugin
