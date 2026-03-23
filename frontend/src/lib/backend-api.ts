import { client } from '~/lib/api-gen/client.gen'
import { get, getAuthMe, getHealth } from '~/lib/api-gen/sdk.gen'
import { supabase } from '~/lib/supabase'

let configured = false

function backendBaseUrl() {
  return import.meta.env.VITE_BACKEND_API_URL?.trim() || ''
}

async function accessToken() {
  const {
    data: { session },
    error
  } = await supabase.auth.getSession()

  if (error) {
    return undefined
  }

  return session?.access_token ?? undefined
}

export function configureBackendClient() {
  if (configured) {
    return
  }

  client.setConfig({
    baseUrl: backendBaseUrl(),
    auth: async () => accessToken()
  })
  configured = true
}

export async function backendGetHealth() {
  configureBackendClient()
  const result = await getHealth()

  if (result.error || !result.data) {
    throw new Error(`Backend health check failed (${result.response.status})`)
  }

  return result.data
}

export async function backendGetSystemMeta() {
  configureBackendClient()
  const result = await get()

  if (result.error || !result.data) {
    throw new Error(`Failed to load backend metadata (${result.response.status})`)
  }

  return result.data
}

export async function backendGetMe() {
  configureBackendClient()
  const result = await getAuthMe()

  if (result.error || !result.data) {
    throw new Error(`Failed to load current user (${result.response.status})`)
  }

  return result.data.user
}
