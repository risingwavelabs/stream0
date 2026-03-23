import { Link, createFileRoute } from '@tanstack/react-router'

export const Route = createFileRoute('/_box0/machines')({
  component: MachinesPage,
})

function MachinesPage() {
  return (
    <>
      <div className="page-header">
        <div>
          <h2>Machines</h2>
          <p className="page-subtitle">
            Check machine capacity, workload and agent distribution.
          </p>
        </div>
        <span className="page-pill">Fleet overview</span>
      </div>

      <div className="card">
        <div className="card-header">Connected machines</div>
        <div className="card-body">
          <table className="list-table">
            <thead>
              <tr>
                <th>Machine</th>
                <th>Status</th>
                <th>CPU</th>
                <th>Agents</th>
              </tr>
            </thead>
            <tbody>
              <tr>
                <td>
                  <Link
                    to="/machines/$machineId"
                    params={{ machineId: 'machine-hk-1' }}
                  >
                    machine-hk-1
                  </Link>
                </td>
                <td>
                  <span className="status-dot online" />
                  Online
                </td>
                <td>42%</td>
                <td>7</td>
              </tr>
              <tr>
                <td>
                  <Link
                    to="/machines/$machineId"
                    params={{ machineId: 'machine-sh-2' }}
                  >
                    machine-sh-2
                  </Link>
                </td>
                <td>
                  <span className="status-dot pending" />
                  Busy
                </td>
                <td>76%</td>
                <td>11</td>
              </tr>
              <tr>
                <td>
                  <Link
                    to="/machines/$machineId"
                    params={{ machineId: 'machine-sz-4' }}
                  >
                    machine-sz-4
                  </Link>
                </td>
                <td>
                  <span className="status-dot stopped" />
                  Offline
                </td>
                <td>-</td>
                <td>0</td>
              </tr>
            </tbody>
          </table>
        </div>
      </div>
    </>
  )
}
