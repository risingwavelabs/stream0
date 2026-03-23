import type { CreateClientConfig } from '@/api-gen/client.gen'

export const createClientConfig: CreateClientConfig = (override = {}) => {
  return {
    ...override,
    baseUrl: import.meta.env.VITE_API_BASE_URL || 'http://localhost:3000'
  }
}
