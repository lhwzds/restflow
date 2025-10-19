use crate::{
    AppCore,
    models::{Node, Task, TaskStatus},
};
use anyhow::{Context, Result};
use serde_json::Value;
use std::sync::Arc;

pub async fn get_task_status(core: &Arc<AppCore>, task_id: &str) -> Result<Task> {
    core.executor
        .get_task_status(task_id)
        .await
        .with_context(|| format!("Failed to get status for task {}", task_id))
}

pub async fn get_execution_status(core: &Arc<AppCore>, execution_id: &str) -> Result<Vec<Task>> {
    core.executor
        .get_execution_status(execution_id)
        .await
        .with_context(|| format!("Failed to get execution status for {}", execution_id))
}

pub async fn list_tasks(
    core: &Arc<AppCore>,
    execution_id: Option<String>,
    status: Option<TaskStatus>,
    limit: Option<u32>,
) -> Result<Vec<Task>> {
    if let Some(exec_id) = execution_id {
        let mut tasks = core
            .executor
            .get_execution_status(&exec_id)
            .await
            .with_context(|| format!("Failed to get tasks for execution {}", exec_id))?;

        if let Some(status_filter) = status {
            tasks.retain(|t| t.status == status_filter);
        }

        if let Some(limit) = limit {
            tasks.truncate(limit as usize);
        }

        Ok(tasks)
    } else {
        let mut tasks = core
            .executor
            .list_tasks(None, status)
            .await
            .context("Failed to list tasks")?;

        if let Some(limit) = limit {
            tasks.truncate(limit as usize);
        }

        Ok(tasks)
    }
}

pub async fn execute_node(core: &Arc<AppCore>, node: Node, input: Value) -> Result<String> {
    core.executor
        .submit_node(node, input)
        .await
        .context("Failed to execute node")
}

const MAX_PAGE_SIZE: usize = 100;

pub async fn list_execution_history(
    core: &Arc<AppCore>,
    workflow_id: &str,
    page: usize,
    page_size: usize,
) -> Result<crate::models::ExecutionHistoryPage> {
    let page = if page == 0 { 1 } else { page };
    let page_size = page_size.clamp(1, MAX_PAGE_SIZE);

    let tasks = core
        .executor
        .list_tasks(Some(workflow_id), None)
        .await
        .with_context(|| format!("Failed to list tasks for workflow {}", workflow_id))?;

    Ok(aggregate_execution_history(workflow_id, tasks, page, page_size))
}

fn aggregate_execution_history(
    workflow_id: &str,
    tasks: Vec<crate::models::Task>,
    page: usize,
    page_size: usize,
) -> crate::models::ExecutionHistoryPage {
    use crate::models::{ExecutionHistoryPage, ExecutionSummary};
    use std::collections::HashMap;

    let mut executions: HashMap<String, Vec<crate::models::Task>> = HashMap::new();
    for task in tasks {
        executions
            .entry(task.execution_id.clone())
            .or_default()
            .push(task);
    };

    let mut summaries: Vec<ExecutionSummary> = executions
        .into_iter()
        .map(|(execution_id, tasks)| {
            ExecutionSummary::from_tasks(execution_id, workflow_id.to_string(), &tasks)
        })
        .collect();

    summaries.sort_by(|a, b| b.started_at.cmp(&a.started_at));

    let total = summaries.len();
    let total_pages = if total == 0 {
        0
    } else {
        ((total - 1) / page_size) + 1
    };

    let current_page = if total_pages == 0 {
        1
    } else {
        page.min(total_pages)
    };

    let start_index = (current_page - 1).saturating_mul(page_size);
    let items: Vec<ExecutionSummary> = if start_index >= total {
        Vec::new()
    } else {
        summaries
            .into_iter()
            .skip(start_index)
            .take(page_size)
            .collect()
    };

    ExecutionHistoryPage {
        items,
        total,
        page: current_page,
        page_size,
        total_pages,
    }
}

#[cfg(test)]
mod tests {
    use super::aggregate_execution_history;
    use crate::engine::context::ExecutionContext;
    use crate::models::{Task, TaskStatus};
    use serde_json::Value;

    fn build_task(
        execution_id: &str,
        workflow_id: &str,
        node_id: &str,
        created_at: i64,
        status: TaskStatus,
    ) -> Task {
        let context = ExecutionContext::new(execution_id.to_string());
        let mut task = Task::new(
            execution_id.to_string(),
            workflow_id.to_string(),
            node_id.to_string(),
            Value::Null,
            context,
        );
        task.created_at = created_at;
        task.started_at = Some(created_at);
        task.completed_at = Some(created_at + 100);
        task.status = status;
        task
    }

    fn build_tasks(total_execs: usize) -> Vec<Task> {
        let mut tasks = Vec::new();
        for i in 0..total_execs {
            let exec_id = format!("exec-{i}");
            // primary task
            tasks.push(build_task(
                &exec_id,
                "wf-1",
                &format!("node-{i}-a"),
                10_000 - (i as i64) * 10,
                TaskStatus::Completed,
            ));
            // secondary task to ensure grouping works
            tasks.push(build_task(
                &exec_id,
                "wf-1",
                &format!("node-{i}-b"),
                10_000 - (i as i64) * 10 + 1,
                TaskStatus::Completed,
            ));
        }
        tasks
    }

    #[test]
    fn paginate_execution_history_first_page() {
        let tasks = build_tasks(5);
        let page = aggregate_execution_history("wf-1", tasks, 1, 2);

        assert_eq!(page.total, 5);
        assert_eq!(page.page, 1);
        assert_eq!(page.page_size, 2);
        assert_eq!(page.total_pages, 3);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].execution_id, "exec-0");
        assert_eq!(page.items[1].execution_id, "exec-1");
    }

    #[test]
    fn paginate_execution_history_middle_page() {
        let tasks = build_tasks(5);
        let page = aggregate_execution_history("wf-1", tasks, 2, 2);

        assert_eq!(page.page, 2);
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].execution_id, "exec-2");
        assert_eq!(page.items[1].execution_id, "exec-3");
    }

    #[test]
    fn paginate_execution_history_out_of_range() {
        let tasks = build_tasks(3);
        let page = aggregate_execution_history("wf-1", tasks, 5, 2);

        assert_eq!(page.total, 3);
        assert_eq!(page.total_pages, 2);
        assert_eq!(page.page, 2);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].execution_id, "exec-2");
    }
}
