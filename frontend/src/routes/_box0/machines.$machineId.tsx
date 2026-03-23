import { createFileRoute, Link } from '@tanstack/react-router'

export const Route = createFileRoute('/_box0/machines/$machineId')({
  component: MachineDetailPage,
})

function MachineDetailPage() {
  const { machineId } = Route.useParams()
  return (
    <>
      <div className="route-back-row">
        <Link to="/machines" className="route-back-link">
          &larr; Machines
        </Link>
      </div>
      <div className="page-header">
        <div>
          <h2>{decodeURIComponent(machineId)}</h2>
          <p className="page-subtitle">
            Machine health snapshot and hosted agent distribution.
          </p>
        </div>
      </div>
      <div className="card">
        <div className="card-header">Machine status</div>
        <div className="card-body">
          <dl className="detail-grid">
            <dt>ID</dt>
            <dd className="mono-detail">{decodeURIComponent(machineId)}</dd>
            <dt>Status</dt>
            <dd>
              <span className="status-dot online" />
              Online
            </dd>
            <dt>CPU usage</dt>
            <dd>42%</dd>
            <dt>Memory</dt>
            <dd>61%</dd>
          </dl>
        </div>
      </div>
      <div className="card">
        <div className="card-header">Agents on this machine</div>
        <div className="card-body">
          <p className="muted-copy">
            Detail view to match the reference HTML app.
          </p>
        </div>
      </div>
    </>
  )
}
