//! # codocia
//!
//! Engine task handling coordinates task records, runs, and agent execution.
//!
//! ## Owns
//! - task run start
//! - task request execution
//! - run status finalization
//!
//! ## Must Not
//! - schedule background workers
//! - own task transport APIs
//! - render task UI
//!
//! ## Inputs
//! - task records
//! - run ids
//! - task requests
//!
//! ## Outputs
//! - run records
//! - agent run output
//!
//! ## Depends On
//! - agent
//! - run
//! - store
//!
//! ## Verify
//! - cargo test -p engine core_run_task_marks_run_done

use anyhow::Result;
use store::Repository;

use crate::Core;

impl Core {
    pub async fn start_run(
        &self,
        task: run::Task,
        run_id: impl Into<String>,
        session_id: impl Into<String>,
    ) -> Result<run::Run> {
        let run = run::Run::new(run_id, task.id.clone())
            .with_session(session_id)
            .with_status(run::Status::Running);
        run::save_task(&self.tasks, task).await?;
        run::save_run(&self.runs, run.clone()).await?;
        Ok(run)
    }

    pub async fn run_task(
        &self,
        run_id: &str,
        request: run::TaskRequest,
    ) -> Result<agent::RunOutput> {
        run::save_task(&self.tasks, request.task.clone()).await?;
        let run = self
            .runs
            .get(run_id)
            .await?
            .unwrap_or_else(|| run::Run::new(run_id, request.task.id.clone()));
        let catalog = self.skill_catalog().await?;
        let input = run::build_agent_input(&catalog, &request);
        let output = agent::Exec::new(self.agent.clone(), self.tools.clone()).dry_run(input);
        run::save_run(&self.runs, run.with_status(run::Status::Done)).await?;
        Ok(output)
    }
}
