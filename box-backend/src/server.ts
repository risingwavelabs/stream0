import { existsSync } from 'node:fs'

import { buildApp } from './app'
import { loadEnv } from './config/env'

async function start() {
  if (existsSync('.env')) {
    process.loadEnvFile('.env')
  }

  const env = loadEnv()
  const app = buildApp({ env })

  const close = async (signal: string) => {
    app.log.info({ signal }, 'shutting down')
    await app.close()
    process.exit(0)
  }

  process.once('SIGINT', () => {
    void close('SIGINT')
  })

  process.once('SIGTERM', () => {
    void close('SIGTERM')
  })

  try {
    await app.listen({
      host: env.HOST,
      port: env.PORT
    })
  } catch (error) {
    app.log.error(error, 'failed to start server')
    process.exit(1)
  }
}

void start()
