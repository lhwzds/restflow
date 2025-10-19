use crate::engine::context::ExecutionContext;
use crate::engine::scheduler::Scheduler;
use crate::models::Node;
use crate::storage::Storage;
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

const QUEUE_POLL_INTERVAL_MS: u64 = 100;

pub struct WorkflowExecutor {
    storage: Arc<Storage>,
    scheduler: Arc<Scheduler>,
    num_workers: usize,
    registry: Arc<crate::node::registry::NodeRegistry>,
    running: Arc<Mutex<bool>>,
}

impl WorkflowExecutor {
    /// Create an asynchronous executor with storage and workers
    pub fn new(
        storage: Arc<Storage>,
        num_workers: usize,
        registry: Arc<crate::node::registry::NodeRegistry>,
    ) -> Self {
        let scheduler = Arc::new(Scheduler::new(storage.queue.clone(), storage.clone()));

        Self {
            storage,
            scheduler,
            num_workers,
            registry,
            running: Arc::new(Mutex::new(false)),
        }
    }

    pub async fn submit(&self, workflow_id: String, input: Value) -> Result<String> {
        self.submit_async(workflow_id, input).await
    }

    pub async fn submit_with_execution_id(
        &self,
        workflow_id: String,
        input: Value,
        execution_id: String,
    ) -> Result<String> {
        self.submit_async_with_id(workflow_id, input, execution_id)
            .await
    }

    /// Submit a single node for execution
    pub async fn submit_node(&self, node: Node, input: Value) -> Result<String> {
        self.scheduler
            .push_single_node(node, input)
            .map_err(|e| anyhow::anyhow!("Failed to submit node: {}", e))
    }

    pub async fn start(&self) {
        if !self.try_start().await {
            return;
        }

        self.recover_stalled_tasks();
        self.spawn_workers(self.num_workers).await;
    }

    async fn submit_async(&self, workflow_id: String, input: Value) -> Result<String> {
        self.scheduler.submit_workflow_by_id(&workflow_id, input)
    }

    async fn submit_async_with_id(
        &self,
        workflow_id: String,
        input: Value,
        execution_id: String,
    ) -> Result<String> {
        self.scheduler
            .submit_workflow_by_id_with_execution_id(&workflow_id, input, execution_id)
    }

    async fn try_start(&self) -> bool {
        let mut running = self.running.lock().await;
        if *running {
            return false;
        }
        *running = true;
        true
    }

    fn recover_stalled_tasks(&self) {
        if let Err(e) = self.scheduler.recover_stalled_tasks() {
            error!(error = %e, "Failed to recover stalled tasks");
        }
    }

    async fn spawn_workers(&self, num_workers: usize) {
        info!(num_workers, "Starting workers");

        for worker_id in 0..num_workers {
            let registry = self.registry.clone();
            let running = self.running.clone();

            let worker = Worker::new(
                worker_id,
                self.storage.clone(),
                self.scheduler.clone(),
                registry,
                running,
            );

            tokio::spawn(async move {
                worker.run_worker_loop().await;
            });
        }
    }

    /// Interpolates {{...}} templates in node config using execution context,
    /// then delegates to node-specific executor for actual execution.
    async fn execute_node(
        node: &Node,
        context: &mut ExecutionContext,
        registry: Arc<crate::node::registry::NodeRegistry>,
    ) -> Result<Value> {
        debug!(node_id = %node.id, node_type = ?node.node_type, "Executing node");

        let executor = registry.get(&node.node_type).ok_or_else(|| {
            anyhow::anyhow!("No executor found for node type: {:?}", node.node_type)
        })?;

        let config = context.interpolate_value(&node.config);
        executor.execute(&config, context).await
    }

    pub async fn get_task_status(&self, task_id: &str) -> Result<crate::models::Task> {
        self.scheduler
            .get_task(task_id)
            .map_err(|e| anyhow::anyhow!("Failed to get task status: {}", e))?
            .ok_or_else(|| anyhow::anyhow!("Task {} not found", task_id))
    }

    pub async fn get_execution_status(
        &self,
        execution_id: &str,
    ) -> Result<Vec<crate::models::Task>> {
        self.scheduler
            .get_tasks_by_execution(execution_id)
            .map_err(|e| anyhow::anyhow!("Failed to get execution status: {}", e))
    }

    pub async fn list_tasks(
        &self,
        workflow_id: Option<&str>,
        status: Option<crate::models::TaskStatus>,
    ) -> Result<Vec<crate::models::Task>> {
        self.scheduler
            .list_tasks(workflow_id, status)
            .map_err(|e| anyhow::anyhow!("Failed to list tasks: {}", e))
    }
}

struct Worker {
    id: usize,
    storage: Arc<Storage>,
    scheduler: Arc<Scheduler>,
    registry: Arc<crate::node::registry::NodeRegistry>,
    running: Arc<Mutex<bool>>,
}

impl Worker {
    fn new(
        id: usize,
        storage: Arc<Storage>,
        scheduler: Arc<Scheduler>,
        registry: Arc<crate::node::registry::NodeRegistry>,
        running: Arc<Mutex<bool>>,
    ) -> Self {
        Self {
            id,
            storage,
            scheduler,
            registry,
            running,
        }
    }

    async fn run_worker_loop(&self) {
        info!(worker_id = self.id, "Worker started");

        while *self.running.lock().await {
            if let Err(e) = self.process_next_task().await {
                let error_msg = e.to_string();
                if !error_msg.contains("Failed to get task") {
                    error!(worker_id = self.id, error = %error_msg, "Worker error");
                }
                tokio::time::sleep(tokio::time::Duration::from_millis(QUEUE_POLL_INTERVAL_MS))
                    .await;
            }
        }

        info!(worker_id = self.id, "Worker stopped");
    }

    async fn process_next_task(&self) -> Result<()> {
        let task = self
            .scheduler
            .pop_task()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to get task: {}", e))?;

        debug!(worker_id = self.id, task_id = %task.id, node_id = %task.node_id, "Processing task");

        let node = task.get_node(&self.storage)?;

        let mut context = task.context.clone();
        context.ensure_secret_storage(&self.storage);
        let result =
            WorkflowExecutor::execute_node(node, &mut context, self.registry.clone()).await;

        match result {
            Ok(output) => match self.scheduler.push_downstream_tasks(&task, output.clone()) {
                Ok(_) => {
                    if let Err(e) = self.scheduler.complete_task(&task.id, output) {
                        warn!(task_id = %task.id, error = %e, "Failed to persist task completion");
                    } else {
                        info!(task_id = %task.id, node_id = %task.node_id, "Task completed");
                    }
                }
                Err(e) => {
                    let error_msg = format!("Task succeeded but failed to push downstream: {}", e);
                    if let Err(persist_err) = self.scheduler.fail_task(&task.id, error_msg.clone())
                    {
                        warn!(task_id = %task.id, error = %persist_err, "Failed to persist task failure");
                    }
                    error!(task_id = %task.id, error = %e, "Failed to push downstream tasks");
                    return Err(anyhow::anyhow!(error_msg));
                }
            },
            Err(error) => {
                if let Err(e) = self.scheduler.fail_task(&task.id, error.to_string()) {
                    warn!(task_id = %task.id, error = %e, "Failed to persist task failure");
                }
                error!(task_id = %task.id, error = %error, "Task execution failed");
            }
        }

        Ok(())
    }
}
