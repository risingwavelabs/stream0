import { mkdtempSync, rmSync } from 'node:fs'
import { join } from 'node:path'
import { tmpdir } from 'node:os'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'

const supabaseMocks = vi.hoisted(() => {
  const anonAuth = {
    signInWithPassword: vi.fn(),
    signInWithOtp: vi.fn(),
    verifyOtp: vi.fn(),
    refreshSession: vi.fn(),
    getClaims: vi.fn()
  }
  const anonClient = { auth: anonAuth }
  const adminClient = { auth: {} }
  const createClient = vi.fn((_url: string, key: string) => {
    return key === 'test-service-role-key' ? adminClient : anonClient
  })

  return {
    anonAuth,
    createClient
  }
})

vi.mock('@supabase/supabase-js', () => ({
  createClient: supabaseMocks.createClient
}))

import { buildApp } from '../src/app'
import { buildEnv } from '../src/config/env'

function buildTestApp() {
  return buildApp({
    env: buildEnv({
      NODE_ENV: 'test',
      ENABLE_SWAGGER: false
    })
  })
}

function buildSession() {
  return {
    access_token: 'access-token',
    refresh_token: 'refresh-token',
    expires_in: 3600,
    token_type: 'bearer',
    user: {
      id: 'user-1',
      email: 'user@example.com',
      role: 'authenticated'
    }
  }
}

describe('auth routes', () => {
  const apps: ReturnType<typeof buildApp>[] = []

  beforeEach(() => {
    supabaseMocks.createClient.mockClear()
    supabaseMocks.anonAuth.signInWithPassword.mockReset()
    supabaseMocks.anonAuth.signInWithOtp.mockReset()
    supabaseMocks.anonAuth.verifyOtp.mockReset()
    supabaseMocks.anonAuth.refreshSession.mockReset()
    supabaseMocks.anonAuth.getClaims.mockReset()
  })

  afterEach(async () => {
    await Promise.all(apps.splice(0).map((app) => app.close()))
  })

  it('fails startup when required Supabase env vars are missing', () => {
    const originalEnv = {
      SUPABASE_URL: process.env.SUPABASE_URL,
      SUPABASE_ANON_KEY: process.env.SUPABASE_ANON_KEY,
      SUPABASE_SERVICE_ROLE_KEY: process.env.SUPABASE_SERVICE_ROLE_KEY
    }
    const originalCwd = process.cwd()
    const tempDir = mkdtempSync(join(tmpdir(), 'box-backend-env-'))

    delete process.env.SUPABASE_URL
    delete process.env.SUPABASE_ANON_KEY
    delete process.env.SUPABASE_SERVICE_ROLE_KEY

    try {
      process.chdir(tempDir)
      expect(() => buildApp()).toThrow(/SUPABASE_URL/)
    } finally {
      process.chdir(originalCwd)
      rmSync(tempDir, { recursive: true, force: true })

      if (originalEnv.SUPABASE_URL) {
        process.env.SUPABASE_URL = originalEnv.SUPABASE_URL
      }
      if (originalEnv.SUPABASE_ANON_KEY) {
        process.env.SUPABASE_ANON_KEY = originalEnv.SUPABASE_ANON_KEY
      }
      if (originalEnv.SUPABASE_SERVICE_ROLE_KEY) {
        process.env.SUPABASE_SERVICE_ROLE_KEY =
          originalEnv.SUPABASE_SERVICE_ROLE_KEY
      }
    }
  })

  it('logs in with email and password', async () => {
    supabaseMocks.anonAuth.signInWithPassword.mockResolvedValue({
      data: { session: buildSession() },
      error: null
    })

    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'POST',
      url: '/auth/login/password',
      payload: {
        email: 'user@example.com',
        password: 'hunter2'
      }
    })

    expect(response.statusCode).toBe(200)
    expect(response.json()).toEqual({
      accessToken: 'access-token',
      refreshToken: 'refresh-token',
      expiresIn: 3600,
      tokenType: 'bearer',
      user: {
        id: 'user-1',
        email: 'user@example.com',
        role: 'authenticated'
      }
    })
  })

  it('returns a generic 401 for invalid credentials', async () => {
    supabaseMocks.anonAuth.signInWithPassword.mockResolvedValue({
      data: { session: null },
      error: { message: 'Invalid login credentials' }
    })

    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'POST',
      url: '/auth/login/password',
      payload: {
        email: 'user@example.com',
        password: 'wrong'
      }
    })

    expect(response.statusCode).toBe(401)
    expect(response.json()).toMatchObject({
      error: 'Unauthorized',
      message: 'Invalid email or password'
    })
    expect(response.json()).toHaveProperty('requestId')
  })

  it('requests an email OTP without leaking account existence', async () => {
    supabaseMocks.anonAuth.signInWithOtp.mockResolvedValue({
      data: { session: null, user: null },
      error: { message: 'User not found', status: 400 }
    })

    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'POST',
      url: '/auth/login/otp/request',
      payload: {
        email: 'user@example.com'
      }
    })

    expect(response.statusCode).toBe(202)
    expect(response.json()).toEqual({ status: 'sent' })
  })

  it('verifies an email OTP', async () => {
    supabaseMocks.anonAuth.verifyOtp.mockResolvedValue({
      data: { session: buildSession() },
      error: null
    })

    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'POST',
      url: '/auth/login/otp/verify',
      payload: {
        email: 'user@example.com',
        token: '123456'
      }
    })

    expect(response.statusCode).toBe(200)
    expect(response.json()).toMatchObject({
      accessToken: 'access-token',
      refreshToken: 'refresh-token',
      user: {
        id: 'user-1',
        email: 'user@example.com',
        role: 'authenticated'
      }
    })
  })

  it('returns a generic 401 for an invalid OTP', async () => {
    supabaseMocks.anonAuth.verifyOtp.mockResolvedValue({
      data: { session: null },
      error: { message: 'Token has expired or is invalid' }
    })

    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'POST',
      url: '/auth/login/otp/verify',
      payload: {
        email: 'user@example.com',
        token: 'bad-token'
      }
    })

    expect(response.statusCode).toBe(401)
    expect(response.json()).toMatchObject({
      error: 'Unauthorized',
      message: 'Invalid or expired login code'
    })
  })

  it('refreshes a session', async () => {
    supabaseMocks.anonAuth.refreshSession.mockResolvedValue({
      data: { session: buildSession() },
      error: null
    })

    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'POST',
      url: '/auth/refresh',
      payload: {
        refreshToken: 'refresh-token'
      }
    })

    expect(response.statusCode).toBe(200)
    expect(response.json()).toMatchObject({
      accessToken: 'access-token',
      refreshToken: 'refresh-token'
    })
  })

  it('returns 401 when refresh token is invalid', async () => {
    supabaseMocks.anonAuth.refreshSession.mockResolvedValue({
      data: { session: null },
      error: { message: 'Invalid refresh token' }
    })

    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'POST',
      url: '/auth/refresh',
      payload: {
        refreshToken: 'bad-refresh-token'
      }
    })

    expect(response.statusCode).toBe(401)
    expect(response.json()).toMatchObject({
      error: 'Unauthorized',
      message: 'Invalid refresh token'
    })
  })

  it('rejects /auth/me without a bearer token', async () => {
    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'GET',
      url: '/auth/me'
    })

    expect(response.statusCode).toBe(401)
    expect(response.json()).toMatchObject({
      error: 'Unauthorized',
      message: 'Invalid or expired token'
    })
  })

  it('returns the authenticated user from /auth/me', async () => {
    supabaseMocks.anonAuth.getClaims.mockResolvedValue({
      data: {
        claims: {
          sub: 'user-1',
          role: 'authenticated',
          email: 'user@example.com'
        },
        header: {
          alg: 'ES256',
          kid: 'test-kid'
        },
        signature: new Uint8Array()
      },
      error: null
    })

    const app = buildTestApp()
    apps.push(app)

    const response = await app.inject({
      method: 'GET',
      url: '/auth/me',
      headers: {
        authorization: 'Bearer test-access-token'
      }
    })

    expect(response.statusCode).toBe(200)
    expect(response.json()).toEqual({
      user: {
        id: 'user-1',
        email: 'user@example.com',
        role: 'authenticated'
      }
    })
  })

  it('enforces role guards from JWT claims', async () => {
    const app = buildTestApp()
    apps.push(app)
    await app.ready()

    await expect(
      app.authorize(['authenticated'])(
        {
          user: {
            sub: 'user-1',
            role: 'authenticated',
            email: 'user@example.com'
          }
        } as never,
        {} as never
      )
    ).resolves.toBeUndefined()

    await expect(
      app.authorize(['admin'])(
        {
          user: {
            sub: 'user-1',
            role: 'authenticated',
            email: 'user@example.com'
          }
        } as never,
        {} as never
      )
    ).rejects.toMatchObject({
      statusCode: 403,
      message: 'Insufficient role'
    })
  })
})
