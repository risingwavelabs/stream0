import { createFileRoute, Link } from '@tanstack/react-router'

export const Route = createFileRoute('/_box0/tasks/$taskId')({
  component: TaskDetailPage,
})

function TaskDetailPage() {
  const { taskId } = Route.useParams()
  return (
    <>
      <div className="route-back-row">
        <Link to="/tasks" className="route-back-link">
          &larr; Tasks
        </Link>
      </div>
      <div className="page-header">
        <div>
          <h2>Task Detail</h2>
          <p className="page-subtitle">
            Execution metadata and timeline for this task.
          </p>
        </div>
      </div>
      <div className="card">
        <div className="card-header">Task summary</div>
        <div className="card-body">
          <dl className="detail-grid">
            <dt>ID</dt>
            <dd className="mono-detail">{taskId}</dd>
            <dt>Status</dt>
            <dd>
              <span className="status-dot working" />
              Running
            </dd>
            <dt>Workspace</dt>
            <dd>core</dd>
            <dt>Owner</dt>
            <dd>planner</dd>
          </dl>
        </div>
      </div>
    </>
  )
}
