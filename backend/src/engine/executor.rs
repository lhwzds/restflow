use crate::models::{Node, Workflow};
use crate::engine::context::{ExecutionContext, namespace};
use crate::engine::graph::WorkflowGraph;
use crate::engine::scheduler::Scheduler;
use crate::storage::Storage;
use anyhow::Result;
use serde_json::Value;
use std::sync::Arc;
use tokio::task::JoinSet;
use tokio::sync::Mutex;
use tracing::{info, warn, error, debug};
use uuid::Uuid;

const QUEUE_POLL_INTERVAL_MS: u64 = 100;


enum ExecutorInner {
    Sync {
        graph: WorkflowGraph,
        context: ExecutionContext,
    },
    Async {
        storage: Arc<Storage>,
        scheduler: Arc<Scheduler>,
        num_workers: usize,
    },
}

pub struct WorkflowExecutor {
    inner: ExecutorInner,
    registry: Arc<crate::node::registry::NodeRegistry>,
    running: Arc<Mutex<bool>>,
}

impl WorkflowExecutor {
    /// Create a synchronous executor for a specific workflow
    /// Optionally provide storage for secret access
    pub fn new_sync(
        workflow: Workflow,
        storage: Option<Arc<Storage>>,
        registry: Arc<crate::node::registry::NodeRegistry>,
    ) -> Self {
        let graph = WorkflowGraph::from_workflow(&workflow);
        let mut context = ExecutionContext::new(workflow.id.clone());

        // Add secret storage to context if provided
        if let Some(storage) = storage {
            context.ensure_secret_storage(&storage);
        }

        Self {
            inner: ExecutorInner::Sync { graph, context },
            registry,
            running: Arc::new(Mutex::new(false)),
        }
    }
    
    /// Create an asynchronous executor with storage and workers
    pub fn new_async(
        storage: Arc<Storage>,
        num_workers: usize,
        registry: Arc<crate::node::registry::NodeRegistry>,
    ) -> Self {
        let scheduler = Arc::new(Scheduler::new(storage.queue.clone(), storage.clone()));

        Self {
            inner: ExecutorInner::Async {
                storage,
                scheduler,
                num_workers,
            },
            registry,
            running: Arc::new(Mutex::new(false)),
        }
    }
    
    /// Set input for sync execution
    pub fn set_input(&mut self, input: Value) {
        if let ExecutorInner::Sync { ref mut context, .. } = self.inner {
            context.set(namespace::trigger::PAYLOAD, input);
        }
    }
    
    /// Execute workflow synchronously (for sync mode)
    pub async fn execute(&mut self) -> Result<Value> {
        match &self.inner {
            ExecutorInner::Sync { .. } => self.execute_sync().await,
            ExecutorInner::Async { .. } => {
                Err(anyhow::anyhow!("Use submit() for async execution"))
            }
        }
    }
    
    /// Submit workflow for async execution (for async mode)
    pub async fn submit(&self, workflow_id: String, input: Value) -> Result<String> {
        match &self.inner {
            ExecutorInner::Async { .. } => self.submit_async(workflow_id, input).await,
            ExecutorInner::Sync { .. } => {
                Err(anyhow::anyhow!("Use execute() for sync execution"))
            }
        }
    }
    
    /// Submit a single node for execution
    pub async fn submit_node(&self, node: Node, input: Value) -> Result<String> {
        match &self.inner {
            ExecutorInner::Async { scheduler, .. } => {
                scheduler.push_single_node(node, input)
                    .map_err(|e| anyhow::anyhow!("Failed to submit node: {}", e))
            }
            ExecutorInner::Sync { .. } => {
                Err(anyhow::anyhow!("Node submission not supported in sync mode"))
            }
        }
    }
    
    /// Start async workers (for async mode)
    pub async fn start(&self) {
        if let ExecutorInner::Async { num_workers, .. } = &self.inner {
            if !self.try_start().await {
                return;
            }
            
            self.recover_stalled_tasks();
            self.spawn_workers(*num_workers).await;
        }
    }
    
    // ============= Private sync execution methods =============
    
    async fn execute_sync(&mut self) -> Result<Value> {
        let (_graph, groups) = match &self.inner {
            ExecutorInner::Sync { graph, .. } => {
                let groups = graph.get_parallel_groups()?;
                (graph, groups)
            }
            _ => return Err(anyhow::anyhow!("Not in sync mode")),
        };
        
        for (stage, group) in groups.iter().enumerate() {
            self.log_stage_start(stage + 1, group);
            self.execute_parallel_group(group).await?;
        }
        
        self.build_result()
    }
    
    async fn execute_parallel_group(&mut self, group: &[String]) -> Result<()> {
        let mut tasks = JoinSet::new();
        
        let (graph, context) = match &self.inner {
            ExecutorInner::Sync { graph, context } => (graph, context),
            _ => return Err(anyhow::anyhow!("Not in sync mode")),
        };
        
        for node_id in group {
            let node = graph.get_node(node_id)
                .cloned()
                .ok_or_else(|| anyhow::anyhow!("Node {} not found", node_id))?;
            
            self.verify_dependencies(node_id)?;
            
            let context_clone = context.clone();
            let registry = self.registry.clone();
            let node_clone = node.clone();
            
            tasks.spawn(async move {
                let mut ctx = context_clone;
                let node_id = node_clone.id.clone();
                let result = Self::execute_node(&node_clone, &mut ctx, registry).await;
                (node_id, result, ctx)
            });
        }
        
        self.await_and_merge_results(tasks).await
    }
    
    async fn await_and_merge_results(&mut self, mut tasks: JoinSet<(String, Result<Value>, ExecutionContext)>) -> Result<()> {
        while let Some(result) = tasks.join_next().await {
            let (node_id, execution_result, node_context) = result
                .map_err(|e| anyhow::anyhow!("Task join error: {}", e))?;
            
            match execution_result {
                Ok(output) => {
                    if let ExecutorInner::Sync { ref mut context, .. } = self.inner {
                        context.set_node(&node_id, output);
                        self.merge_context(&node_context);
                    }
                    info!(node_id = %node_id, "Node completed");
                }
                Err(err) => {
                    return Err(anyhow::anyhow!("Node {} failed: {}", node_id, err));
                }
            }
        }
        Ok(())
    }
    
    fn merge_context(&mut self, other: &ExecutionContext) {
        if let ExecutorInner::Sync { ref mut context, .. } = self.inner {
            for (key, value) in &other.data {
                // Check for conflicts and warn
                if let Some(existing) = context.get(key) {
                    if existing != value {
                        warn!(
                            key = %key,
                            existing = ?existing,
                            new = ?value,
                            "Context key conflict detected - parallel nodes wrote different values"
                        );
                    }
                }
                context.set(key, value.clone());
            }
        }
    }
    
    fn verify_dependencies(&self, node_id: &str) -> Result<()> {
        let (graph, context) = match &self.inner {
            ExecutorInner::Sync { graph, context } => (graph, context),
            _ => return Err(anyhow::anyhow!("Not in sync mode")),
        };

        for dep_id in graph.get_dependencies(node_id) {
            if context.get_node(&dep_id).is_none() {
                return Err(anyhow::anyhow!("Dependency {} not completed for node {}", dep_id, node_id));
            }
        }
        Ok(())
    }
    
    fn log_stage_start(&self, stage_num: usize, group: &[String]) {
        debug!(stage = stage_num, nodes = ?group, "Executing stage");
    }
    
    fn build_result(&self) -> Result<Value> {
        let context = match &self.inner {
            ExecutorInner::Sync { context, .. } => context,
            _ => return Err(anyhow::anyhow!("Not in sync mode")),
        };

        Ok(serde_json::json!({
            "execution_id": context.execution_id,
            "status": "completed",
            "data": context.data
        }))
    }
    
    // ============= Private async execution methods =============
    
    async fn submit_async(&self, workflow_id: String, input: Value) -> Result<String> {
        let (storage, scheduler) = match &self.inner {
            ExecutorInner::Async { storage, scheduler, .. } => (storage, scheduler),
            _ => return Err(anyhow::anyhow!("Not in async mode")),
        };
        
        // Load workflow
        let workflow = storage.workflows.get_workflow(&workflow_id)
            .map_err(|e| anyhow::anyhow!("Failed to load workflow: {}", e))?;
        
        // Decompose workflow into start node tasks
        let execution_id = Uuid::new_v4().to_string();
        let graph = WorkflowGraph::from_workflow(&workflow);
        let start_nodes = graph.get_nodes_with_no_dependencies();
        
        if start_nodes.is_empty() {
            return Err(anyhow::anyhow!("No start nodes found in workflow"));
        }
        
        // Create initial context
        let mut context = ExecutionContext::new(execution_id.clone());
        context.ensure_secret_storage(&storage);
        context.set(namespace::trigger::PAYLOAD, input.clone());

        // Push all start nodes to queue (including triggers)
        for node_id in start_nodes {
            if let Some(node) = graph.get_node(&node_id) {
                // Nodes reference data via {{...}} templates in config, no need for resolve_node_input
                scheduler.push_task(
                    execution_id.clone(),
                    node.clone(),
                    workflow.clone(),
                    context.clone(),
                    Value::Null  // No longer need to pass input
                ).map_err(|e| anyhow::anyhow!("Failed to queue node: {}", e))?;
            }
        }
        
        Ok(execution_id)
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
        if let ExecutorInner::Async { scheduler, .. } = &self.inner {
            if let Err(e) = scheduler.recover_stalled_tasks() {
                error!(error = %e, "Failed to recover stalled tasks");
            }
        }
    }
    
    async fn spawn_workers(&self, num_workers: usize) {
        info!(num_workers, "Starting workers");

        let (storage, scheduler) = match &self.inner {
            ExecutorInner::Async { storage, scheduler, .. } =>
                (Some(storage.clone()), Some(scheduler.clone())),
            _ => (None, None),
        };
        
        for worker_id in 0..num_workers {
            let registry = self.registry.clone();
            let running = self.running.clone();
            
            if let (Some(storage), Some(scheduler)) = (storage.clone(), scheduler.clone()) {
                let worker = Worker::new(
                    worker_id,
                    storage,
                    scheduler,
                    registry,
                    running
                );
                
                tokio::spawn(async move {
                    worker.run().await;
                });
            }
        }
    }
    
    // ============= Shared node execution logic =============

    /// Unified node execution logic used by both sync and async modes
    async fn execute_node(
        node: &Node,
        context: &mut ExecutionContext,
        registry: Arc<crate::node::registry::NodeRegistry>,
    ) -> Result<Value> {
        debug!(node_id = %node.id, node_type = ?node.node_type, "Executing node");

        let executor = registry.get(&node.node_type)
            .ok_or_else(|| anyhow::anyhow!("No executor found for node type: {:?}", node.node_type))?;

        let config = context.interpolate_value(&node.config);
        executor.execute(&config, context).await
    }
    
    // ============= Task status methods (async mode only) =============
    
    pub async fn get_task_status(&self, task_id: &str) -> Result<crate::models::Task> {
        match &self.inner {
            ExecutorInner::Async { scheduler, .. } => {
                scheduler.get_task(task_id)
                    .map_err(|e| anyhow::anyhow!("Failed to get task status: {}", e))?
                    .ok_or_else(|| anyhow::anyhow!("Task {} not found", task_id))
            }
            _ => Err(anyhow::anyhow!("Task status only available in async mode")),
        }
    }
    
    pub async fn get_execution_status(&self, execution_id: &str) -> Result<Vec<crate::models::Task>> {
        match &self.inner {
            ExecutorInner::Async { scheduler, .. } => {
                scheduler.get_tasks_by_execution(execution_id)
                    .map_err(|e| anyhow::anyhow!("Failed to get execution status: {}", e))
            }
            _ => Err(anyhow::anyhow!("Execution status only available in async mode")),
        }
    }
    
    pub async fn list_tasks(
        &self,
        workflow_id: Option<&str>,
        status: Option<crate::models::TaskStatus>,
    ) -> Result<Vec<crate::models::Task>> {
        match &self.inner {
            ExecutorInner::Async { scheduler, .. } => {
                scheduler.list_tasks(workflow_id, status)
                    .map_err(|e| anyhow::anyhow!("Failed to list tasks: {}", e))
            }
            _ => Err(anyhow::anyhow!("Task listing only available in async mode")),
        }
    }
}

// ============= Worker for async execution =============

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
        running: Arc<Mutex<bool>>
    ) -> Self {
        Self { id, storage, scheduler, registry, running }
    }
    
    async fn run(&self) {
        info!(worker_id = self.id, "Worker started");
        
        while *self.running.lock().await {
            if let Err(e) = self.process_next_task().await {
                // Only log error if it's not a queue empty error
                let error_msg = e.to_string();
                if !error_msg.contains("Failed to get task") {
                    error!(worker_id = self.id, error = %error_msg, "Worker error");
                }
                // Brief sleep to avoid busy waiting when queue is empty
                tokio::time::sleep(tokio::time::Duration::from_millis(QUEUE_POLL_INTERVAL_MS)).await;
            }
        }

        info!(worker_id = self.id, "Worker stopped");
    }
    
    async fn process_next_task(&self) -> Result<()> {
        let task = self.scheduler.pop_task().await
            .map_err(|e| anyhow::anyhow!("Failed to get task: {}", e))?;
        
        debug!(worker_id = self.id, task_id = %task.id, node_id = %task.node_id, "Processing task");
        
        // Get the node from task (lazy-loaded)
        let node = task.get_node(&self.storage)?;
        
        // Execute the node
        let mut context = task.context.clone();
        // Ensure secret_storage is available if not already set
        context.ensure_secret_storage(&self.storage);
        let result = WorkflowExecutor::execute_node(node, &mut context, self.registry.clone()).await;
        
        // Handle the result and queue downstream tasks if successful
        match result {
            Ok(output) => {
                // Queue downstream tasks - if this fails, mark task as failed
                match self.scheduler.queue_downstream_tasks(&task, output.clone()) {
                    Ok(_) => {
                        let _ = self.scheduler.complete_task(&task.id, output);
                        info!(task_id = %task.id, node_id = %task.node_id, "Task completed");
                    }
                    Err(e) => {
                        let error_msg = format!("Task succeeded but failed to queue downstream: {}", e);
                        let _ = self.scheduler.fail_task(&task.id, error_msg.clone());
                        error!(task_id = %task.id, error = %e, "Failed to queue downstream tasks");
                        return Err(anyhow::anyhow!(error_msg));
                    }
                }
            }
            Err(error) => {
                let _ = self.scheduler.fail_task(&task.id, error.to_string());
                error!(task_id = %task.id, error = %error, "Task execution failed");
            }
        }
        
        Ok(())
    }
}
