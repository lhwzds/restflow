use super::*;
use crate::models::{BackgroundAgentRun, BackgroundAgentRunMetrics, BackgroundAgentRunStatus};
use std::collections::BTreeMap;

impl BackgroundAgentStorage {
    fn normalize_checkpoint_id(checkpoint_id: Option<String>) -> Option<String> {
        checkpoint_id.and_then(|value| {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }

    pub fn create_task_run(&self, run: BackgroundAgentRun) -> Result<BackgroundAgentRun> {
        let json_bytes = serde_json::to_vec(&run)?;
        self.inner.put_run_raw_with_status(
            &run.run_id,
            &run.task_id,
            run.status.as_str(),
            &json_bytes,
        )?;
        Ok(run)
    }

    pub fn get_task_run(&self, run_id: &str) -> Result<Option<BackgroundAgentRun>> {
        self.inner
            .get_run_raw(run_id)?
            .map(|bytes| serde_json::from_slice(&bytes).map_err(Into::into))
            .transpose()
    }

    pub fn list_task_runs(&self, task_id: &str) -> Result<Vec<BackgroundAgentRun>> {
        let mut runs = self
            .inner
            .list_runs_by_task_raw(task_id)?
            .into_iter()
            .map(|(_, bytes)| {
                serde_json::from_slice::<BackgroundAgentRun>(&bytes).map_err(anyhow::Error::from)
            })
            .collect::<Result<Vec<_>>>()?;
        runs.sort_by_key(|run| (run.started_at, run.run_id.clone()));
        Ok(runs)
    }

    pub fn list_active_task_runs(&self) -> Result<Vec<BackgroundAgentRun>> {
        let runs = self
            .inner
            .list_runs_raw()?
            .into_iter()
            .map(|(_, bytes)| {
                serde_json::from_slice::<BackgroundAgentRun>(&bytes).map_err(anyhow::Error::from)
            })
            .collect::<Result<Vec<_>>>()?;

        let mut by_task = BTreeMap::<String, Vec<BackgroundAgentRun>>::new();
        for run in runs {
            by_task.entry(run.task_id.clone()).or_default().push(run);
        }

        let mut active_runs = Vec::new();
        for (task_id, task_runs) in by_task {
            if let Some(active) = self.reconcile_active_task_runs(&task_id, task_runs)? {
                active_runs.push(active);
            }
        }

        active_runs.sort_by_key(|run| (run.started_at, run.run_id.clone()));
        Ok(active_runs)
    }

    pub fn get_active_task_run(&self, task_id: &str) -> Result<Option<BackgroundAgentRun>> {
        self.reconcile_active_task_runs(task_id, self.list_task_runs(task_id)?)
    }

    pub fn start_task_run(
        &self,
        task_id: &str,
        run_id: impl Into<String>,
        execution_id: impl Into<String>,
        started_at: i64,
        checkpoint_id: Option<String>,
    ) -> Result<BackgroundAgentRun> {
        let run_id = run_id.into();
        let execution_id = execution_id.into();
        let checkpoint_id = Self::normalize_checkpoint_id(checkpoint_id);
        self.validate_checkpoint_ownership(task_id, &execution_id, checkpoint_id.as_deref())?;

        let run = BackgroundAgentRun::new(
            run_id,
            task_id.to_string(),
            execution_id,
            started_at,
            checkpoint_id,
        );
        self.create_task_run(run)
    }

    pub fn set_task_run_checkpoint(
        &self,
        run_id: &str,
        checkpoint_id: Option<String>,
    ) -> Result<Option<BackgroundAgentRun>> {
        let Some(mut run) = self.get_task_run(run_id)? else {
            return Ok(None);
        };

        let checkpoint_id = Self::normalize_checkpoint_id(checkpoint_id);
        self.validate_checkpoint_ownership(
            &run.task_id,
            &run.execution_id,
            checkpoint_id.as_deref(),
        )?;
        let previous_status = run.status.clone();
        run.set_checkpoint_id(checkpoint_id);
        self.update_task_run(&run, previous_status)?;
        Ok(Some(run))
    }

    pub fn mark_task_run_terminal(
        &self,
        run_id: &str,
        status: BackgroundAgentRunStatus,
        ended_at: i64,
        error: Option<String>,
        metrics: BackgroundAgentRunMetrics,
    ) -> Result<Option<BackgroundAgentRun>> {
        let Some(mut run) = self.get_task_run(run_id)? else {
            return Ok(None);
        };

        let previous_status = run.status.clone();
        run.mark_terminal(status, ended_at, error, metrics);
        self.update_task_run(&run, previous_status)?;
        Ok(Some(run))
    }

    pub fn interrupt_task_run(
        &self,
        run_id: &str,
        ended_at: i64,
        reason: impl Into<String>,
    ) -> Result<Option<BackgroundAgentRun>> {
        let Some(mut run) = self.get_task_run(run_id)? else {
            return Ok(None);
        };

        let previous_status = run.status.clone();
        run.mark_interrupted(ended_at, reason);
        self.update_task_run(&run, previous_status)?;
        Ok(Some(run))
    }

    fn update_task_run(
        &self,
        run: &BackgroundAgentRun,
        previous_status: BackgroundAgentRunStatus,
    ) -> Result<()> {
        let json_bytes = serde_json::to_vec(run)?;
        self.inner.update_run_raw_with_status(
            &run.run_id,
            &run.task_id,
            previous_status.as_str(),
            run.status.as_str(),
            &json_bytes,
        )
    }

    fn refresh_active_task_run_index(&self, run: &BackgroundAgentRun) -> Result<()> {
        let json_bytes = serde_json::to_vec(run)?;
        self.inner.update_run_raw_with_status(
            &run.run_id,
            &run.task_id,
            run.status.as_str(),
            run.status.as_str(),
            &json_bytes,
        )
    }

    fn reconcile_active_task_runs(
        &self,
        task_id: &str,
        runs: Vec<BackgroundAgentRun>,
    ) -> Result<Option<BackgroundAgentRun>> {
        let mut active_runs = runs
            .into_iter()
            .filter(|run| run.status.is_active())
            .collect::<Vec<_>>();

        if active_runs.is_empty() {
            self.inner.clear_active_run_raw(task_id)?;
            return Ok(None);
        }

        active_runs.sort_by_key(|run| (run.started_at, run.run_id.clone()));
        let winner = active_runs
            .pop()
            .expect("active run collection must be non-empty");

        if !active_runs.is_empty() {
            let recovered_at = chrono::Utc::now().timestamp_millis();
            let reason = format!("Recovered duplicate active run; kept '{}'", winner.run_id);
            for loser in active_runs {
                self.interrupt_task_run(&loser.run_id, recovered_at, reason.clone())?;
            }
        }

        self.refresh_active_task_run_index(&winner)?;
        Ok(Some(winner))
    }
}
