import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_box0/users')({
  component: UsersPage,
})

function UsersPage() {
  return (
    <>
      <div className="page-header">
        <div>
          <h2>Users</h2>
          <p className="page-subtitle">
            Team members, role boundaries, and access footprint.
          </p>
        </div>
        <span className="page-pill">Admin panel</span>
      </div>

      <div className="card">
        <div className="card-header">Organization users</div>
        <div className="card-body">
          <table className="list-table">
            <thead>
              <tr>
                <th>User</th>
                <th>Role</th>
                <th>Workspaces</th>
                <th>Last seen</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>alice@box0.dev</td>
                <td>Admin</td>
                <td>core, infra</td>
                <td>1m ago</td>
              </tr>
              <tr>
                <td>bob@box0.dev</td>
                <td>Operator</td>
                <td>ops</td>
                <td>12m ago</td>
              </tr>
              <tr>
                <td>carol@box0.dev</td>
                <td>Viewer</td>
                <td>core</td>
                <td>1h ago</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  )
}
