import fp from 'fastify-plugin'
import type { FastifyPluginAsyncTypebox } from '@fastify/type-provider-typebox'
import { createClient } from '@supabase/supabase-js'

type AuthenticatedUser = {
  sub: string
  role: string
  email: string | null
  exp?: number
  iat?: number
}

type SupabaseJwtPayload = {
  sub?: unknown
  role?: unknown
  email?: unknown
  exp?: unknown
  iat?: unknown
}

function buildStatusError(
  statusCode: number,
  name: string,
  message: string
): Error & { statusCode: number } {
  return Object.assign(new Error(message), {
    name,
    statusCode
  })
}

function formatJwtUser(payload: SupabaseJwtPayload): AuthenticatedUser {
  const user: AuthenticatedUser = {
    sub: typeof payload.sub === 'string' ? payload.sub : '',
    role: typeof payload.role === 'string' ? payload.role : '',
    email: typeof payload.email === 'string' ? payload.email : null
  }

  if (typeof payload.exp === 'number') {
    user.exp = payload.exp
  }

  if (typeof payload.iat === 'number') {
    user.iat = payload.iat
  }

  return user
}

function getBearerToken(authorizationHeader: string | undefined) {
  if (!authorizationHeader) {
    return null
  }

  const [scheme, token] = authorizationHeader.split(' ')

  if (scheme !== 'Bearer' || !token) {
    return null
  }

  return token
}

const supabasePlugin: FastifyPluginAsyncTypebox = async (app) => {
  const supabaseAnon = createClient(
    app.env.SUPABASE_URL,
    app.env.SUPABASE_ANON_KEY,
    {
      auth: {
        autoRefreshToken: false,
        persistSession: false,
        detectSessionInUrl: false
      }
    }
  )

  const supabaseAdmin = createClient(
    app.env.SUPABASE_URL,
    app.env.SUPABASE_SERVICE_ROLE_KEY,
    {
      auth: {
        autoRefreshToken: false,
        persistSession: false,
        detectSessionInUrl: false
      }
    }
  )

  app.decorate('supabaseAnon', supabaseAnon)
  app.decorate('supabaseAdmin', supabaseAdmin)
  app.decorateRequest('user', null as unknown as AuthenticatedUser)

  app.decorate('authenticate', async function authenticate(request) {
    const token = getBearerToken(request.headers.authorization)

    if (!token) {
      throw buildStatusError(401, 'Unauthorized', 'Invalid or expired token')
    }

    const { data, error } = await app.supabaseAnon.auth.getClaims(token)

    if (error || !data) {
      throw buildStatusError(401, 'Unauthorized', 'Invalid or expired token')
    }

    request.user = formatJwtUser(data.claims)

    if (!request.user.sub || !request.user.role) {
      throw buildStatusError(401, 'Unauthorized', 'Invalid or expired token')
    }
  })

  app.decorate('authorize', function authorize(roles) {
    return async function authorizer(request) {
      if (roles.length > 0 && !roles.includes(request.user.role)) {
        throw buildStatusError(403, 'Forbidden', 'Insufficient role')
      }
    }
  })
}

export default fp(supabasePlugin, {
  name: 'supabase'
})
