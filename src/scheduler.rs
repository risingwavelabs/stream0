use std::time::Duration;

use crate::db::CronJob;
use crate::server::SharedState;

const SCHEDULER_INTERVAL_SECS: u64 = 60;
const TEMP_AGENT_MAX_AGE_SECS: i64 = 86400; // 24 hours
const WORKFLOW_RUN_TIMEOUT_SECS: i64 = 3600; // 1 hour

/// Parse a schedule string and return the interval in seconds.
/// Supported formats:
///   "30s", "5m", "1h", "6h", "1d"
///   "every 30s", "every 5m", "every 1h", "every 6h", "every 1d"
pub fn parse_schedule_secs(schedule: &str) -> Option<u64> {
    let s = schedule
        .trim()
        .strip_prefix("every ")
        .unwrap_or(schedule)
        .trim();
    let (num_str, unit) = if s.ends_with('s') {
        (&s[..s.len() - 1], 's')
    } else if s.ends_with('m') {
        (&s[..s.len() - 1], 'm')
    } else if s.ends_with('h') {
        (&s[..s.len() - 1], 'h')
    } else if s.ends_with('d') {
        (&s[..s.len() - 1], 'd')
    } else {
        return None;
    };
    let num: u64 = num_str.parse().ok()?;
    let secs = match unit {
        's' => num,
        'm' => num * 60,
        'h' => num * 3600,
        'd' => num * 86400,
        _ => return None,
    };
    if secs == 0 {
        return None;
    }
    Some(secs)
}

fn should_run(job: &CronJob) -> bool {
    let interval_secs = match parse_schedule_secs(&job.schedule) {
        Some(s) => s,
        None => return false,
    };

    match job.last_run {
        None => true,
        Some(last) => {
            let elapsed = (chrono::Utc::now() - last).num_seconds();
            elapsed >= interval_secs as i64
        }
    }
}

/// Scheduler loop. Runs inside the server process, checks cron jobs every minute.
pub async fn run(state: SharedState) {
    tracing::info!("Scheduler started");
    let interval = Duration::from_secs(SCHEDULER_INTERVAL_SECS);

    loop {
        tokio::time::sleep(interval).await;

        // Clean up expired temp agents
        match state
            .db
            .cleanup_expired_temp_agents(TEMP_AGENT_MAX_AGE_SECS)
        {
            Ok(n) if n > 0 => tracing::info!("Cleaned up {} expired temp agents", n),
            Err(e) => tracing::error!("Failed to clean up temp agents: {}", e),
            _ => {}
        }

        // Time out stale workflow runs
        match state.db.get_stale_workflow_runs(WORKFLOW_RUN_TIMEOUT_SECS) {
            Ok(runs) => {
                for run in runs {
                    tracing::warn!(
                        run_id = run.id,
                        workspace = run.workspace_name,
                        "Scheduler: timing out stale workflow run"
                    );
                    if let Err(e) = state.db.timeout_workflow_run(&run.workspace_name, &run.id) {
                        tracing::error!("Failed to timeout workflow run {}: {}", run.id, e);
                    }
                }
            }
            Err(e) => tracing::error!("Failed to check stale workflow runs: {}", e),
        }

        let jobs = match state.db.get_all_enabled_cron_jobs() {
            Ok(j) => j,
            Err(e) => {
                tracing::error!("Scheduler: failed to get cron jobs: {}", e);
                continue;
            }
        };

        for job in jobs {
            // Check end_date: disable expired crons
            if let Some(end) = job.end_date {
                if chrono::Utc::now() > end {
                    tracing::info!(cron_id = job.id, "Scheduler: cron job expired, disabling");
                    let _ = state.db.set_cron_enabled(&job.workspace_name, &job.id, false);
                    continue;
                }
            }

            if !should_run(&job) {
                continue;
            }

            tracing::info!(
                cron_id = job.id,
                agent = job.agent,
                workspace = job.workspace_name,
                "Scheduler: triggering cron job"
            );

            // Create inbox message (same as b0 delegate)
            let lead_id = format!("cron-{}", &job.id[..job.id.len().min(12)]);
            let thread_id = format!("thread-{}", &uuid::Uuid::new_v4().to_string()[..8]);
            let _ = state.db.send_inbox_message(
                &job.workspace_name,
                &thread_id,
                &lead_id,
                &job.agent,
                "request",
                Some(&serde_json::json!(job.task)),
            );

            // Update last_run
            let _ = state.db.update_cron_last_run(&job.id);
        }
    }
}
