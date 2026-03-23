import { Link, createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_box0/agents')({
  component: AgentsPage,
})

function AgentsPage() {
  return (
    <>
      <div className="page-header">
        <div>
          <h2>Agents</h2>
          <p className="page-subtitle">
            Monitor agent health, ownership, and conversation load.
          </p>
        </div>
        <span className="page-pill">Agent directory</span>
      </div>

      <div className="card">
        <div className="card-header">Registered agents</div>
        <div className="card-body">
          <table className="list-table">
            <thead>
              <tr>
                <th>Name</th>
                <th>Status</th>
                <th>Machine</th>
                <th>Last activity</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>
                  <Link to="/agents/$name" params={{ name: 'planner' }}>
                    planner
                  </Link>
                </td>
                <td>
                  <span className="status-dot active" />
                  Active
                </td>
                <td>machine-hk-1</td>
                <td>10s ago</td>
              </tr>
              <tr>
                <td>
                  <Link to="/agents/$name" params={{ name: 'reviewer' }}>
                    reviewer
                  </Link>
                </td>
                <td>
                  <span className="status-dot pending" />
                  Waiting
                </td>
                <td>machine-sh-2</td>
                <td>2m ago</td>
              </tr>
              <tr>
                <td>
                  <Link to="/agents/$name" params={{ name: 'builder' }}>
                    builder
                  </Link>
                </td>
                <td>
                  <span className="status-dot online" />
                  Online
                </td>
                <td>machine-hz-3</td>
                <td>4m ago</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  )
}
