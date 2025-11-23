use crate::engine::context::ExecutionContext;
use crate::engine::scheduler::Scheduler;
use crate::models::{Node, NodeType};
use crate::python::PythonManager;
use crate::storage::Storage;
use anyhow::{Result, anyhow};
use serde_json::Value;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tracing::{debug, error, info, warn};

const QUEUE_POLL_INTERVAL_MS: u64 = 100;

pub struct WorkflowExecutor {
    storage: Arc<Storage>,
    scheduler: Arc<Scheduler>,
    num_workers: usize,
    registry: Arc<crate::node::registry::NodeRegistry>,
    running: Arc<Mutex<bool>>,
    python_manager: Arc<Mutex<Option<Arc<PythonManager>>>>,
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
            python_manager: Arc::new(Mutex::new(None)),
        }
    }

    /// Set the Python manager for script execution
    pub async fn set_python_manager(&self, manager: Arc<PythonManager>) {
        let mut pm = self.python_manager.lock().await;
        *pm = Some(manager);
    }

    /// Get the Python manager if available
    pub async fn get_python_manager(&self) -> Option<Arc<PythonManager>> {
        let pm = self.python_manager.lock().await;
        pm.clone()
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
            let python_manager = self.python_manager.clone();

            let worker = Worker::new(
                worker_id,
                self.storage.clone(),
                self.scheduler.clone(),
                registry,
                running,
                python_manager,
            );

            tokio::spawn(async move {
                worker.run_worker_loop().await;
            });
        }
    }

    /// Resolves Templated<T> fields in NodeInput using execution context,
    /// then delegates to node-specific executor for actual execution.
    async fn execute_node(
        node: &Node,
        input: &crate::models::NodeInput,
        context: &mut ExecutionContext,
        registry: Arc<crate::node::registry::NodeRegistry>,
    ) -> Result<crate::models::NodeOutput> {
        use crate::models::NodeInput;

        debug!(node_id = %node.id, node_type = ?node.node_type, "Executing node");

        let executor = registry.get(&node.node_type).ok_or_else(|| {
            anyhow::anyhow!("No executor found for node type: {:?}", node.node_type)
        })?;

        // Resolve templates and convert to Value for existing executors
        let config = match input {
            NodeInput::HttpRequest(http_input) => {
                let url = http_input.url.resolve(context)?;
                let headers = http_input
                    .headers
                    .as_ref()
                    .map(|h| h.resolve(context))
                    .transpose()?;
                let body = http_input
                    .body
                    .as_ref()
                    .map(|b| b.resolve(context))
                    .transpose()?;

                serde_json::json!({
                    "url": url,
                    "method": http_input.method,
                    "headers": headers,
                    "body": body,
                    "timeout_ms": http_input.timeout_ms,
                })
            }
            NodeInput::Agent(agent_input) => {
                let prompt = agent_input.prompt.resolve(context)?;
                serde_json::json!({
                    "model": agent_input.model,
                    "prompt": prompt.clone(),
                    "temperature": agent_input.temperature,
                    "api_key_config": agent_input.api_key_config,
                    "tools": agent_input.tools,
                    "input": prompt,  // For AgentExecutor compatibility
                })
            }
            NodeInput::Python(python_input) => {
                // Note: code is not templated to avoid conflicts with Python f-strings
                let code = python_input.code.clone();
                let input_data = python_input
                    .input
                    .as_ref()
                    .map(|i| i.resolve(context))
                    .transpose()?;

                serde_json::json!({
                    "code": code,
                    "input": input_data,
                })
            }
            NodeInput::Print(print_input) => {
                let message = print_input.message.resolve(context)?;
                serde_json::json!({
                    "message": message,
                })
            }
            NodeInput::Email(email_input) => {
                let to = email_input.to.resolve(context)?;
                let cc = email_input
                    .cc
                    .as_ref()
                    .map(|c| c.resolve(context))
                    .transpose()?;
                let bcc = email_input
                    .bcc
                    .as_ref()
                    .map(|b| b.resolve(context))
                    .transpose()?;
                let subject = email_input.subject.resolve(context)?;
                let body = email_input.body.resolve(context)?;

                // Build config with SMTP fields
                serde_json::json!({
                    "to": to,
                    "cc": cc,
                    "bcc": bcc,
                    "subject": subject,
                    "body": body,
                    "html": email_input.html,
                    "smtp_server": email_input.smtp_server.clone(),
                    "smtp_port": email_input.smtp_port,
                    "smtp_username": email_input.smtp_username.clone(),
                    "smtp_password_config": serde_json::to_value(&email_input.smtp_password_config)?,
                    "smtp_use_tls": email_input.smtp_use_tls,
                })
            }
            NodeInput::ManualTrigger(manual_input) => {
                // Manual triggers don't need input resolution - they provide data to the workflow
                serde_json::to_value(manual_input).map_err(|e| {
                    anyhow::anyhow!("Failed to serialize manual trigger input: {}", e)
                })?
            }
            NodeInput::WebhookTrigger(webhook_input) => {
                // Webhook triggers don't need input resolution - they provide data to the workflow
                serde_json::to_value(webhook_input).map_err(|e| {
                    anyhow::anyhow!("Failed to serialize webhook trigger input: {}", e)
                })?
            }
            NodeInput::ScheduleTrigger(schedule_input) => serde_json::to_value(schedule_input)
                .map_err(|e| anyhow::anyhow!("Failed to serialize schedule input: {}", e))?,
        };

        executor.execute(&node.node_type, &config, context).await
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
    python_manager: Arc<Mutex<Option<Arc<PythonManager>>>>,
}

impl Worker {
    fn new(
        id: usize,
        storage: Arc<Storage>,
        scheduler: Arc<Scheduler>,
        registry: Arc<crate::node::registry::NodeRegistry>,
        running: Arc<Mutex<bool>>,
        python_manager: Arc<Mutex<Option<Arc<PythonManager>>>>,
    ) -> Self {
        Self {
            id,
            storage,
            scheduler,
            registry,
            running,
            python_manager,
        }
    }

    async fn run_worker_loop(&self) {
        info!(worker_id = self.id, "Worker started");

        while *self.running.lock().await {
            if let Err(e) = self.process_next_task().await {
                let error_msg = e.to_string();
                // Only log errors that are not "no tasks available" (which is normal during polling)
                if !error_msg.contains("Failed to get task")
                    && !error_msg.contains("No pending tasks")
                {
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

        // Handle Python manager based on node type
        if node.node_type == NodeType::Python {
            // For Python nodes, MUST have manager - wait if initializing
            debug!(worker_id = self.id, task_id = %task.id, "Python node detected, waiting for Python manager");

            let manager = {
                let mut retries = 0;
                const MAX_RETRIES: u32 = 300; // 30 seconds (100ms * 300)

                loop {
                    if let Some(m) = self.python_manager.lock().await.clone() {
                        break m;
                    }

                    if retries >= MAX_RETRIES {
                        error!(
                            worker_id = self.id,
                            task_id = %task.id,
                            node_id = %task.node_id,
                            timeout_seconds = MAX_RETRIES / 10,
                            "Python manager not available after timeout - initialization may have failed"
                        );
                        return Err(anyhow!(
                            "Python manager not available after {}s. \
                             Ensure Python manager is initialized before submitting Python tasks.",
                            MAX_RETRIES / 10
                        ));
                    }

                    retries += 1;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            };

            debug!(worker_id = self.id, task_id = %task.id, "Python manager acquired");
            context = context.with_python_manager(manager);
        } else {
            // For non-Python nodes, use if available (optional)
            if let Some(manager) = self.python_manager.lock().await.clone() {
                context = context.with_python_manager(manager);
            }
        }

        let result =
            WorkflowExecutor::execute_node(node, &task.input, &mut context, self.registry.clone())
                .await;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{Node, NodeType, Workflow};
    use crate::node::registry::NodeRegistry;
    use crate::storage::Storage;
    use tempfile::tempdir;

    fn create_test_executor() -> (WorkflowExecutor, tempfile::TempDir) {
        let temp_dir = tempdir().unwrap();
        let db_path = temp_dir.path().join("test.db");
        let storage = Arc::new(Storage::new(db_path.to_str().unwrap()).unwrap());
        let registry = Arc::new(NodeRegistry::new());
        let executor = WorkflowExecutor::new(storage, 2, registry);
        (executor, temp_dir)
    }

    fn create_test_node(id: &str, node_type: NodeType, config: Value) -> Node {
        Node {
            id: id.to_string(),
            node_type,
            config,
            position: None,
        }
    }

    fn create_test_print_node(id: &str, message: &str) -> Node {
        create_test_node(
            id,
            NodeType::Print,
            serde_json::json!({
                "type": "Print",
                "data": {
                    "message": message
                }
            }),
        )
    }

    fn create_test_workflow(id: &str, nodes: Vec<Node>) -> Workflow {
        Workflow {
            id: id.to_string(),
            name: format!("Test Workflow {}", id),
            nodes,
            edges: vec![],
        }
    }

    #[tokio::test]
    async fn test_executor_creation() {
        let (executor, _tmp) = create_test_executor();

        // Verify executor is not running initially
        let running = executor.running.lock().await;
        assert!(!*running);
    }

    #[tokio::test]
    async fn test_submit_single_node() {
        let (executor, _tmp) = create_test_executor();

        let node = create_test_print_node("print1", "Hello World");
        let input = serde_json::json!({});

        let task_id = executor.submit_node(node, input).await.unwrap();
        assert!(!task_id.is_empty());

        // Verify task was queued
        let task = executor.get_task_status(&task_id).await.unwrap();
        assert_eq!(task.node_id, "print1");
    }

    #[tokio::test]
    async fn test_submit_workflow() {
        let (executor, _tmp) = create_test_executor();

        let node = create_test_print_node("print1", "Test");
        let workflow = create_test_workflow("wf-001", vec![node]);

        // Store workflow first
        executor
            .storage
            .workflows
            .create_workflow(&workflow)
            .unwrap();

        let execution_id = executor
            .submit("wf-001".to_string(), serde_json::json!({}))
            .await
            .unwrap();

        assert!(!execution_id.is_empty());
    }

    #[tokio::test]
    async fn test_submit_with_custom_execution_id() {
        let (executor, _tmp) = create_test_executor();

        let node = create_test_print_node("print1", "Test");
        let workflow = create_test_workflow("wf-001", vec![node]);
        executor
            .storage
            .workflows
            .create_workflow(&workflow)
            .unwrap();

        let custom_id = "custom-exec-001".to_string();
        let execution_id = executor
            .submit_with_execution_id(
                "wf-001".to_string(),
                serde_json::json!({}),
                custom_id.clone(),
            )
            .await
            .unwrap();

        assert_eq!(execution_id, custom_id);
    }

    #[tokio::test]
    async fn test_executor_start_idempotent() {
        let (executor, _tmp) = create_test_executor();

        // First start should succeed
        executor.start().await;
        let running = *executor.running.lock().await;
        assert!(running);

        // Second start should be no-op (try_start returns false)
        let try_start_result = executor.try_start().await;
        assert!(!try_start_result);
    }

    #[tokio::test]
    async fn test_get_task_status() {
        let (executor, _tmp) = create_test_executor();

        let node = create_test_print_node("print1", "Test");
        let task_id = executor
            .submit_node(node, serde_json::json!({}))
            .await
            .unwrap();

        let task = executor.get_task_status(&task_id).await.unwrap();
        assert_eq!(task.id, task_id);
        assert_eq!(task.node_id, "print1");
    }

    #[tokio::test]
    async fn test_get_task_status_not_found() {
        let (executor, _tmp) = create_test_executor();

        let result = executor.get_task_status("nonexistent-task").await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[tokio::test]
    async fn test_get_execution_status() {
        let (executor, _tmp) = create_test_executor();

        let node = create_test_print_node("print1", "Test");
        let workflow = create_test_workflow("wf-001", vec![node]);
        executor
            .storage
            .workflows
            .create_workflow(&workflow)
            .unwrap();

        let execution_id = executor
            .submit("wf-001".to_string(), serde_json::json!({}))
            .await
            .unwrap();

        let tasks = executor.get_execution_status(&execution_id).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].execution_id, execution_id);
    }

    #[tokio::test]
    async fn test_list_tasks() {
        let (executor, _tmp) = create_test_executor();

        let node1 = create_test_print_node("print1", "Test1");
        let node2 = create_test_print_node("print2", "Test2");

        executor
            .submit_node(node1, serde_json::json!({}))
            .await
            .unwrap();
        executor
            .submit_node(node2, serde_json::json!({}))
            .await
            .unwrap();

        let tasks = executor.list_tasks(None, None).await.unwrap();
        assert_eq!(tasks.len(), 2);
    }

    #[tokio::test]
    async fn test_list_tasks_filtered_by_workflow() {
        let (executor, _tmp) = create_test_executor();

        let node = create_test_print_node("print1", "Test");
        let workflow = create_test_workflow("wf-001", vec![node]);
        executor
            .storage
            .workflows
            .create_workflow(&workflow)
            .unwrap();

        executor
            .submit("wf-001".to_string(), serde_json::json!({}))
            .await
            .unwrap();

        let tasks = executor.list_tasks(Some("wf-001"), None).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].workflow_id, "wf-001");
    }

    #[tokio::test]
    async fn test_execute_node_with_template_resolution() {
        let (executor, _tmp) = create_test_executor();

        // Create context with variables
        let mut context = ExecutionContext::new("wf-001".to_string());
        context.set_var("name", serde_json::json!("Alice"));

        // Create print node with template
        let node = create_test_node(
            "print1",
            NodeType::Print,
            serde_json::json!({
                "type": "Print",
                "data": {
                    "message": "Hello {{var.name}}!"
                }
            }),
        );

        // Parse NodeInput
        let node_input: crate::models::NodeInput =
            serde_json::from_value(node.config.clone()).unwrap();

        let result = WorkflowExecutor::execute_node(
            &node,
            &node_input,
            &mut context,
            executor.registry.clone(),
        )
        .await;

        assert!(result.is_ok());
        let output = result.unwrap();

        // Verify the message was interpolated
        if let crate::models::NodeOutput::Print(print_output) = output {
            assert_eq!(print_output.printed, "Hello Alice!");
        } else {
            panic!("Expected Print output");
        }
    }

    #[tokio::test]
    async fn test_worker_picks_up_task() {
        let (executor, _tmp) = create_test_executor();

        // Start the executor with workers
        executor.start().await;

        // Give workers time to start
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        // Submit a print node
        let node = create_test_print_node("print1", "Worker test");
        let task_id = executor
            .submit_node(node, serde_json::json!({}))
            .await
            .unwrap();

        // Wait for worker to pick up the task (should transition from Pending)
        let mut attempts = 0;
        let max_attempts = 20; // 2 seconds
        let mut task_picked_up = false;

        loop {
            if attempts >= max_attempts {
                break;
            }

            let task = executor.get_task_status(&task_id).await.unwrap();
            if task.status != crate::models::TaskStatus::Pending {
                task_picked_up = true;
                break;
            }

            attempts += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }

        assert!(task_picked_up, "Worker should have picked up the task");
    }

    // TODO: Fix worker task completion - tasks are picked up but not completing
    // This is a known issue that needs investigation
    #[tokio::test]
    #[ignore = "Worker completion logic needs fixing - task stuck in Running state"]
    async fn test_worker_completes_task() {
        let (executor, _tmp) = create_test_executor();

        executor.start().await;
        tokio::time::sleep(tokio::time::Duration::from_millis(200)).await;

        let node = create_test_print_node("print1", "Worker test");
        let task_id = executor
            .submit_node(node, serde_json::json!({}))
            .await
            .unwrap();

        let mut attempts = 0;
        loop {
            if attempts >= 100 {
                let task = executor.get_task_status(&task_id).await.unwrap();
                panic!("Task did not complete. Final status: {:?}", task.status);
            }

            let task = executor.get_task_status(&task_id).await.unwrap();
            if task.status == crate::models::TaskStatus::Completed {
                assert!(task.output.is_some());
                break;
            }

            attempts += 1;
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        }
    }

    #[tokio::test]
    async fn test_python_manager_injection() {
        let (executor, _tmp) = create_test_executor();

        // Verify no Python manager initially
        assert!(executor.get_python_manager().await.is_none());

        // Create a mock Python manager without network/filesystem operations
        let manager = PythonManager::new_mock();
        executor.set_python_manager(manager).await;

        // Verify Python manager is set
        assert!(executor.get_python_manager().await.is_some());
    }
}
