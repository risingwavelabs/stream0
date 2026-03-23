import { writeFileSync } from 'node:fs'
import { resolve } from 'node:path'
import { buildApp } from './app'

async function main() {
  const app = buildApp({
    env: {
      NODE_ENV: 'development',
      HOST: '0.0.0.0',
      PORT: 3000,
      LOG_LEVEL: 'silent',
      CORS_ORIGIN: '*',
      ENABLE_SWAGGER: true,
      SUPABASE_URL: 'http://localhost',
      SUPABASE_ANON_KEY: 'placeholder',
      SUPABASE_SERVICE_ROLE_KEY: 'placeholder'
    }
  })

  await app.ready()

  const spec = JSON.stringify(app.swagger(), null, 2)
  const outputPath = resolve(__dirname, '../../frontend/swagger.json')
  writeFileSync(outputPath, spec)

  console.log(`OpenAPI spec written to ${outputPath}`)
  await app.close()
}

main().catch((err) => {
  console.error(err)
  process.exit(1)
})
