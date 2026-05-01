//! # codocia
//!
//! Engine snapshot handling imports and exports protocol-safe core state.
//!
//! ## Owns
//! - CoreSnapshot import
//! - CoreSnapshot export
//! - snapshot-to-store replay order
//!
//! ## Must Not
//! - read legacy databases directly
//! - own migration DTOs
//! - own transport encoding
//!
//! ## Inputs
//! - CoreSnapshot
//! - engine stores
//!
//! ## Outputs
//! - restored Core
//! - exported CoreSnapshot
//!
//! ## Depends On
//! - proto
//! - chat
//! - run
//! - store
//!
//! ## Verify
//! - cargo test -p engine core_snapshot_exports_and_imports_state

use anyhow::Result;
use proto::CoreSnapshot;
use store::Repository;

use crate::Core;

impl Core {
    pub async fn from_snapshot(snapshot: CoreSnapshot) -> Result<Self> {
        let mut core = Self::new(snapshot.current_model);
        for spec in snapshot.models {
            core.insert_model(spec);
        }
        for skill in snapshot.skills {
            core.save_skill(skill).await?;
        }
        for session in snapshot.sessions {
            core.save_session(session).await?;
        }
        for task in snapshot.tasks {
            core.save_task(task).await?;
        }
        for run in snapshot.runs {
            core.save_run(run).await?;
        }
        for profile in snapshot.profiles {
            core.save_profile(profile).await?;
        }
        Ok(core)
    }

    pub async fn snapshot(&self) -> Result<CoreSnapshot> {
        Ok(CoreSnapshot {
            current_model: self.agent.model.clone(),
            models: self.models.list().into_iter().cloned().collect(),
            skills: self.skills.list().await?,
            sessions: self.sessions.list().await?,
            tasks: self.tasks.list().await?,
            runs: self.runs.list().await?,
            profiles: self.profiles.list().await?,
            tool_specs: self.tools.specs(),
        })
    }
}
