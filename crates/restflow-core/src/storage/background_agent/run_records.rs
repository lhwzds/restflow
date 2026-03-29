use super::*;
use crate::models::{BackgroundAgentRun, BackgroundAgentRunMetrics, BackgroundAgentRunStatus};

impl BackgroundAgentStorage {
    pub fn create_task_run(&self, run: BackgroundAgentRun) -> Result<BackgroundAgentRun> {
        let json_bytes = serde_json::to_vec(&run)?;
        self.inner.put_run_raw(&run.run_id, &run.task_id, &json_bytes)?;
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
        let mut runs = self
            .inner
            .list_runs_raw()?
            .into_iter()
            .map(|(_, bytes)| {
                serde_json::from_slice::<BackgroundAgentRun>(&bytes).map_err(anyhow::Error::from)
            })
            .collect::<Result<Vec<_>>>()?;
        runs.retain(|run| run.status.is_active());
        runs.sort_by_key(|run| (run.started_at, run.run_id.clone()));
        Ok(runs)
    }

    pub fn get_active_task_run(&self, task_id: &str) -> Result<Option<BackgroundAgentRun>> {
        let mut runs = self.list_task_runs(task_id)?;
        runs.retain(|run| run.status.is_active());
        Ok(runs.into_iter().max_by_key(|run| (run.started_at, run.run_id.clone())))
    }

    pub fn start_task_run(
        &self,
        task_id: &str,
        run_id: impl Into<String>,
        execution_id: impl Into<String>,
        started_at: i64,
        checkpoint_id: Option<String>,
    ) -> Result<BackgroundAgentRun> {
        let run = BackgroundAgentRun::new(run_id, task_id.to_string(), execution_id, started_at, checkpoint_id);
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
        run.set_checkpoint_id(checkpoint_id);
        self.update_task_run(&run)?;
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
        run.mark_terminal(status, ended_at, error, metrics);
        self.update_task_run(&run)?;
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
        run.mark_interrupted(ended_at, reason);
        self.update_task_run(&run)?;
        Ok(Some(run))
    }

    fn update_task_run(&self, run: &BackgroundAgentRun) -> Result<()> {
        let json_bytes = serde_json::to_vec(run)?;
        self.inner.update_run_raw(&run.run_id, &run.task_id, &json_bytes)
    }
}
