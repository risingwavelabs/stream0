import { defineConfig } from '@hey-api/openapi-ts'

export default defineConfig({
  input: '../box-backend/openapi/swagger.json',
  output: {
    path: './src/lib/api-gen',
    clean: true
  },
  plugins: ['@hey-api/client-fetch']
})
