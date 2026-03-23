import { createFileRoute, Link } from '@tanstack/react-router'

export const Route = createFileRoute('/_box0/agents/$name')({
  component: AgentDetailPage,
})

function AgentDetailPage() {
  const { name } = Route.useParams()
  return (
    <>
      <div className="route-back-row">
        <Link to="/agents" className="route-back-link">
          &larr; Agents
        </Link>
      </div>
      <div className="page-header">
        <div>
          <h2>{decodeURIComponent(name)}</h2>
          <p className="page-subtitle">
            Profile, status and recent conversation context.
          </p>
        </div>
      </div>
      <div className="card">
        <div className="card-header">Agent overview</div>
        <div className="card-body">
          <dl className="detail-grid">
            <dt>Name</dt>
            <dd className="mono-detail">{decodeURIComponent(name)}</dd>
            <dt>Status</dt>
            <dd>
              <span className="status-dot active" />
              Active
            </dd>
            <dt>Current workspace</dt>
            <dd>core</dd>
            <dt>Current machine</dt>
            <dd>machine-hk-1</dd>
          </dl>
        </div>
      </div>
      <div className="card">
        <div className="card-header">Conversations</div>
        <div className="card-body">
          <p className="muted-copy">
            Thread list and inbox UI to be ported from the reference dashboard.
          </p>
        </div>
      </div>
    </>
  )
}
