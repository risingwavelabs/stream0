import { mkdir, writeFile } from 'node:fs/promises'
import { dirname, resolve } from 'node:path'

import { buildApp } from '../src/app'
import { buildEnv } from '../src/config/env'

async function generateSwagger() {
  const outputPath = resolve(process.cwd(), 'openapi', 'swagger.json')
  const app = buildApp({
    env: buildEnv({
      NODE_ENV: 'development',
      ENABLE_SWAGGER: true
    })
  })

  await app.ready()

  const swagger = app.swagger()

  await mkdir(dirname(outputPath), {
    recursive: true
  })
  await writeFile(outputPath, `${JSON.stringify(swagger, null, 2)}\n`, 'utf8')

  await app.close()
  // eslint-disable-next-line no-console
  console.log(`OpenAPI spec generated at ${outputPath}`)
}

void generateSwagger().catch(async (error: unknown) => {
  // eslint-disable-next-line no-console
  console.error('Failed to generate OpenAPI spec', error)
  process.exitCode = 1
})
