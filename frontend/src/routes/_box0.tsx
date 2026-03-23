import {
  Link,
  Outlet,
  createFileRoute,
  redirect,
  useNavigate,
} from '@tanstack/react-router'
import * as React from 'react'
import {
  apiGet,
  getAccessToken,
  getStoredWorkspace,
  signOut,
  setStoredWorkspace,
} from '~/lib/box0-api'

type WorkspacesResponse = { workspaces?: { name: string }[] }

export const Route = createFileRoute('/_box0')({
  beforeLoad: async () => {
    if (typeof window === 'undefined') return

    const token = await getAccessToken()
    if (!token) throw redirect({ to: '/login' })
  },
  component: Box0Layout,
})

function Box0Layout() {
  const navigate = useNavigate()
  const [workspaces, setWorkspaces] = React.useState<{ name: string }[]>([])
  const [workspace, setWorkspace] = React.useState<string>(() => {
    return getStoredWorkspace() || ''
  })

  React.useEffect(() => {
    let active = true

    const loadWorkspaces = async () => {
      try {
        const data = await apiGet<WorkspacesResponse>('/workspaces')
        if (!active) return

        const list = data.workspaces || []
        setWorkspaces(list)
        const saved = getStoredWorkspace()
        if (saved && list.some((w) => w.name === saved)) {
          setWorkspace(saved)
        } else if (list[0]) {
          setWorkspace(list[0].name)
          setStoredWorkspace(list[0].name)
        }
      } catch (error) {
        const message =
          error instanceof Error ? error.message.toLowerCase() : ''

        // During auth migration we may still hit legacy endpoints that expect X-API-Key.
        // Do not force sign-out for this case on initial page entry.
        if (message.includes('missing x-api-key header')) {
          return
        }

        await signOut()
        if (!active) return
        navigate({ to: '/login' })
      }
    }

    void loadWorkspaces()

    return () => {
      active = false
    }
  }, [navigate])

  const onWorkspaceChange = (name: string) => {
    setWorkspace(name)
    setStoredWorkspace(name)
  }

  return (
    <div className="app-layout">
      <nav className="sidebar">
        <div className="sidebar-logo">
          <div className="sidebar-logo-mark">B0</div>
          <div>
            <div className="sidebar-logo-title">Box0</div>
            <div className="sidebar-logo-subtitle">Operations Hub</div>
          </div>
        </div>
        <div className="sidebar-section-title">Primary</div>
        <div className="sidebar-nav">
          <Link
            to="/tasks"
            activeOptions={{ exact: false }}
            activeProps={{ className: 'active' }}
            className="sidebar-link"
          >
            <span className="nav-icon">▦</span> Tasks
          </Link>
        </div>
        <div className="sidebar-section-title">Resources</div>
        <div className="sidebar-nav">
          <Link
            to="/agents"
            className="sidebar-link"
            activeProps={{ className: 'active' }}
          >
            <span className="nav-icon">◉</span> Agents
          </Link>
          <Link
            to="/machines"
            className="sidebar-link"
            activeProps={{ className: 'active' }}
          >
            <span className="nav-icon">◫</span> Machines
          </Link>
          <Link
            to="/users"
            className="sidebar-link"
            activeProps={{ className: 'active' }}
          >
            <span className="nav-icon">◌</span> Users
          </Link>
        </div>
        <div className="sidebar-group">
          <label>Workspace</label>
          <div className="sidebar-workspace-row">
            <select
              value={workspace}
              onChange={(e) => onWorkspaceChange(e.target.value)}
            >
              {workspaces.map((w) => (
                <option key={w.name} value={w.name}>
                  {w.name}
                </option>
              ))}
            </select>
            <Link
              to="/workspaces"
              title="Manage workspaces"
              className="sidebar-settings-link"
            >
              &#9881;
            </Link>
          </div>
        </div>
        <div className="sidebar-footer">
          <div className="user-name">Signed in with Supabase</div>
          <button
            type="button"
            className="btn btn-outline btn-sm sidebar-signout"
            onClick={() => {
              void signOut().finally(() => {
                navigate({ to: '/login' })
              })
            }}
          >
            Sign out
          </button>
        </div>
      </nav>
      <main className="main-content">
        <Outlet />
      </main>
    </div>
  )
}
