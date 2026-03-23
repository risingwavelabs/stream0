import 'fastify'

import type { AppEnv } from '../config/env'
import type { FastifyReply, FastifyRequest } from 'fastify'
import type { SupabaseClient } from '@supabase/supabase-js'

type AuthenticatedUser = {
  sub: string
  role: string
  email: string | null
  exp?: number
  iat?: number
}

declare module 'fastify' {
  interface FastifyInstance {
    env: AppEnv
    supabaseAnon: SupabaseClient
    supabaseAdmin: SupabaseClient
    authenticate: (
      request: FastifyRequest,
      reply: FastifyReply
    ) => Promise<void>
    authorize: (
      roles: string[]
    ) => (request: FastifyRequest, reply: FastifyReply) => Promise<void>
  }
  interface FastifyRequest {
    user: AuthenticatedUser
  }
}
