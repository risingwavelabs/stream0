import { Type } from '@sinclair/typebox'
import type { Session, User } from '@supabase/supabase-js'
import type { FastifyPluginAsyncTypebox } from '@fastify/type-provider-typebox'

const AuthUserSchema = Type.Object({
  id: Type.String(),
  email: Type.Union([Type.String(), Type.Null()]),
  role: Type.String()
})

const TokenResponseSchema = Type.Object({
  accessToken: Type.String(),
  refreshToken: Type.String(),
  expiresIn: Type.Number(),
  tokenType: Type.String(),
  user: AuthUserSchema
})

const OtpRequestResponseSchema = Type.Object({
  status: Type.Literal('sent')
})

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

function toAuthUser(user: Pick<User, 'id' | 'email' | 'role'>) {
  return {
    id: user.id,
    email: user.email ?? null,
    role: user.role ?? 'authenticated'
  }
}

function toTokenResponse(session: Session) {
  return {
    accessToken: session.access_token,
    refreshToken: session.refresh_token,
    expiresIn: session.expires_in,
    tokenType: session.token_type,
    user: toAuthUser(session.user)
  }
}

function requireSession(session: Session | null) {
  if (!session) {
    throw new Error('Supabase auth did not return a session')
  }

  return session
}

const authRoutes: FastifyPluginAsyncTypebox = async (app) => {
  app.post(
    '/auth/login/password',
    {
      schema: {
        tags: ['auth'],
        body: Type.Object({
          email: Type.String({ minLength: 1 }),
          password: Type.String({ minLength: 1 })
        }),
        response: {
          200: TokenResponseSchema
        }
      }
    },
    async (request) => {
      const { data, error } = await app.supabaseAnon.auth.signInWithPassword(
        request.body
      )

      if (error) {
        throw buildStatusError(401, 'Unauthorized', 'Invalid email or password')
      }

      return toTokenResponse(requireSession(data.session))
    }
  )

  app.post(
    '/auth/login/otp/request',
    {
      schema: {
        tags: ['auth'],
        body: Type.Object({
          email: Type.String({ minLength: 1 })
        }),
        response: {
          202: OtpRequestResponseSchema
        }
      }
    },
    async (request, reply) => {
      const { error } = await app.supabaseAnon.auth.signInWithOtp({
        email: request.body.email,
        options: {
          shouldCreateUser: false
        }
      })

      if (error && (typeof error.status !== 'number' || error.status >= 500)) {
        throw buildStatusError(502, 'Bad Gateway', 'Failed to send login code')
      }

      return reply.status(202).send({ status: 'sent' as const })
    }
  )

  app.post(
    '/auth/login/otp/verify',
    {
      schema: {
        tags: ['auth'],
        body: Type.Object({
          email: Type.String({ minLength: 1 }),
          token: Type.String({ minLength: 1 })
        }),
        response: {
          200: TokenResponseSchema
        }
      }
    },
    async (request) => {
      const { data, error } = await app.supabaseAnon.auth.verifyOtp({
        email: request.body.email,
        token: request.body.token,
        type: 'email'
      })

      if (error) {
        throw buildStatusError(
          401,
          'Unauthorized',
          'Invalid or expired login code'
        )
      }

      return toTokenResponse(requireSession(data.session))
    }
  )

  app.post(
    '/auth/refresh',
    {
      schema: {
        tags: ['auth'],
        body: Type.Object({
          refreshToken: Type.String({ minLength: 1 })
        }),
        response: {
          200: TokenResponseSchema
        }
      }
    },
    async (request) => {
      const { data, error } = await app.supabaseAnon.auth.refreshSession({
        refresh_token: request.body.refreshToken
      })

      if (error) {
        throw buildStatusError(401, 'Unauthorized', 'Invalid refresh token')
      }

      return toTokenResponse(requireSession(data.session))
    }
  )

  app.get(
    '/auth/me',
    {
      onRequest: [app.authenticate],
      schema: {
        tags: ['auth'],
        security: [
          {
            bearerAuth: []
          }
        ],
        response: {
          200: Type.Object({
            user: AuthUserSchema
          })
        }
      }
    },
    async (request) => ({
      user: {
        id: request.user.sub,
        email: request.user.email,
        role: request.user.role
      }
    })
  )
}

export default authRoutes
