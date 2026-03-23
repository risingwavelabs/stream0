import { Link, createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_box0/tasks')({
  component: TasksPage,
})

function TasksPage() {
  return (
    <>
      <div className="page-header">
        <div>
          <h2>Tasks</h2>
          <p className="page-subtitle">
            Track task execution, review status, and jump into details.
          </p>
        </div>
        <span className="page-pill">Live queue preview</span>
      </div>

      <div className="dashboard-kpi-grid">
        <div className="dashboard-kpi-card">
          <div className="dashboard-kpi-label">Queued</div>
          <div className="dashboard-kpi-value">12</div>
          <div className="dashboard-kpi-note">Awaiting assignment</div>
        </div>
        <div className="dashboard-kpi-card">
          <div className="dashboard-kpi-label">Running</div>
          <div className="dashboard-kpi-value">4</div>
          <div className="dashboard-kpi-note">Active on 2 machines</div>
        </div>
        <div className="dashboard-kpi-card">
          <div className="dashboard-kpi-label">Finished Today</div>
          <div className="dashboard-kpi-value">37</div>
          <div className="dashboard-kpi-note">92% success rate</div>
        </div>
      </div>

      <div className="card">
        <div className="card-header">Recent tasks</div>
        <div className="card-body">
          <table className="list-table">
            <thead>
              <tr>
                <th>Task</th>
                <th>Status</th>
                <th>Workspace</th>
                <th>Updated</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>
                  <Link to="/tasks/$taskId" params={{ taskId: 'task-98f1' }}>
                    task-98f1
                  </Link>
                </td>
                <td>
                  <span className="status-dot working" />
                  Running
                </td>
                <td>core</td>
                <td>20s ago</td>
              </tr>
              <tr>
                <td>
                  <Link to="/tasks/$taskId" params={{ taskId: 'task-97ab' }}>
                    task-97ab
                  </Link>
                </td>
                <td>
                  <span className="status-dot done" />
                  Done
                </td>
                <td>infra</td>
                <td>3m ago</td>
              </tr>
              <tr>
                <td>
                  <Link to="/tasks/$taskId" params={{ taskId: 'task-96de' }}>
                    task-96de
                  </Link>
                </td>
                <td>
                  <span className="status-dot pending" />
                  Pending
                </td>
                <td>ops</td>
                <td>8m ago</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  )
}
