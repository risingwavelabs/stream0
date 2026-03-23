import { defineConfig } from '@hey-api/openapi-ts'

export default defineConfig({
  input: './swagger.json',
  output: {
    path: './src/api-gen',
    clean: true,
    preferExportAll: true,
  },
  plugins: [
    {
      name: '@hey-api/client-ofetch',
      runtimeConfigPath: '@/lib/client.config',
      exportFromIndex: true,
    },
    {
      name: '@tanstack/react-query',
    },
    {
      name: 'zod',
      responses: false,
    },
    {
      name: '@hey-api/sdk',
      validator: true,
    },
  ],
})
