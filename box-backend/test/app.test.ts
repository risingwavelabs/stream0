import { afterEach, describe, expect, it } from 'vitest'

import { buildApp } from '../src/app'
import { buildEnv } from '../src/config/env'

describe('buildApp', () => {
  const apps: ReturnType<typeof buildApp>[] = []

  afterEach(async () => {
    await Promise.all(apps.splice(0).map((app) => app.close()))
  })

  it('serves the health route', async () => {
    const app = buildApp({
      env: buildEnv({
        NODE_ENV: 'test',
        ENABLE_SWAGGER: false
      })
    })
    apps.push(app)

    const response = await app.inject({
      method: 'GET',
      url: '/health'
    })

    expect(response.statusCode).toBe(200)
    expect(response.json()).toEqual({ status: 'ok' })
  })

  it('serves service metadata', async () => {
    const app = buildApp({
      env: buildEnv({
        NODE_ENV: 'test',
        ENABLE_SWAGGER: false
      })
    })
    apps.push(app)

    const response = await app.inject({
      method: 'GET',
      url: '/'
    })

    expect(response.statusCode).toBe(200)
    expect(response.json()).toMatchObject({
      name: 'box-backend',
      version: '1.0.0',
      environment: 'test'
    })
  })

  it('returns a consistent not found error', async () => {
    const app = buildApp({
      env: buildEnv({
        NODE_ENV: 'test',
        ENABLE_SWAGGER: false
      })
    })
    apps.push(app)

    const response = await app.inject({
      method: 'GET',
      url: '/missing'
    })

    expect(response.statusCode).toBe(404)
    expect(response.json()).toMatchObject({
      error: 'Not Found',
      message: 'Route GET /missing not found'
    })
    expect(response.json()).toHaveProperty('requestId')
  })

  it('exposes bearer auth in the OpenAPI spec', async () => {
    const app = buildApp({
      env: buildEnv({
        NODE_ENV: 'test',
        ENABLE_SWAGGER: true
      })
    })
    apps.push(app)
    await app.ready()

    const spec = app.swagger()

    expect(spec.components?.securitySchemes).toMatchObject({
      bearerAuth: {
        type: 'http',
        scheme: 'bearer',
        bearerFormat: 'JWT'
      }
    })
    expect(spec.paths?.['/auth/me']?.get?.security).toEqual([
      {
        bearerAuth: []
      }
    ])
  })
})
