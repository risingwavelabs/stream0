import 'fastify'

import type { AppEnv } from '../config/env'

declare module 'fastify' {
  interface FastifyInstance {
    env: AppEnv
  }
}
