import { supabase } from '~/lib/supabase'

const WORKSPACE_KEY = 'b0_workspace'

export async function getAccessToken(): Promise<string | null> {
  const {
    data: { session },
    error,
  } = await supabase.auth.getSession()

  if (error) return null

  return session?.access_token ?? null
}

export async function signOut() {
  clearStoredAuth()
  await supabase.auth.signOut()
}

export function clearStoredAuth() {
  localStorage.removeItem(WORKSPACE_KEY)
}

export function getStoredWorkspace(): string | null {
  if (typeof localStorage === 'undefined') return null
  return localStorage.getItem(WORKSPACE_KEY)
}

export function setStoredWorkspace(name: string) {
  localStorage.setItem(WORKSPACE_KEY, name)
}

export async function apiHeaders(): Promise<HeadersInit> {
  const h: Record<string, string> = { 'Content-Type': 'application/json' }
  const token = await getAccessToken()
  if (token) h.Authorization = `Bearer ${token}`
  return h
}

export async function apiGet<T = unknown>(path: string): Promise<T> {
  const res = await fetch(path, { headers: await apiHeaders() })

  let data: unknown = null
  try {
    data = await res.json()
  } catch {
    data = null
  }

  if (!res.ok) {
    const errorMessage =
      typeof data === 'object' &&
      data !== null &&
      'error' in data &&
      typeof data.error === 'string'
        ? data.error
        : `Request failed (${res.status})`
    throw new Error(errorMessage)
  }

  return data as T
}
