import { createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_box0/workspaces')({
  component: WorkspacesPage,
})

function WorkspacesPage() {
  return (
    <>
      <div className="page-header">
        <div>
          <h2>Workspaces</h2>
          <p className="page-subtitle">
            Control workspace boundaries, membership, and quota.
          </p>
        </div>
        <span className="page-pill">Workspace manager</span>
      </div>

      <div className="dashboard-kpi-grid">
        <div className="dashboard-kpi-card">
          <div className="dashboard-kpi-label">Total Workspaces</div>
          <div className="dashboard-kpi-value">3</div>
          <div className="dashboard-kpi-note">core, infra, ops</div>
        </div>
        <div className="dashboard-kpi-card">
          <div className="dashboard-kpi-label">Members</div>
          <div className="dashboard-kpi-value">14</div>
          <div className="dashboard-kpi-note">Across all environments</div>
        </div>
        <div className="dashboard-kpi-card">
          <div className="dashboard-kpi-label">Active Tasks</div>
          <div className="dashboard-kpi-value">19</div>
          <div className="dashboard-kpi-note">Running right now</div>
        </div>
      </div>

      <div className="card">
        <div className="card-header">Workspace list</div>
        <div className="card-body">
          <table className="list-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Members</th>
                <th>Agents</th>
                <th>Last update</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>core</td>
                <td>7</td>
                <td>11</td>
                <td>2m ago</td>
              </tr>
              <tr>
                <td>infra</td>
                <td>4</td>
                <td>6</td>
                <td>9m ago</td>
              </tr>
              <tr>
                <td>ops</td>
                <td>3</td>
                <td>4</td>
                <td>16m ago</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  )
}
