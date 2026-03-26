use super::*;

impl RestFlowMcpServer {
    pub(crate) async fn handle_manage_background_agents(
        &self,
        params: ManageBackgroundAgentsParams,
    ) -> Result<String, String> {
        let operation = params.operation.trim().to_lowercase();
        let params = self
            .apply_background_agent_api_defaults(operation.as_str(), params)
            .await?;

        let value = match operation.as_str() {
            "list"
            | "create"
            | "convert_session"
            | "promote_to_background"
            | "update"
            | "delete"
            | "start"
            | "pause"
            | "resume"
            | "run"
            | "stop"
            | "control"
            | "progress"
            | "send_message"
            | "list_messages"
            | "list_deliverables"
            | "run_batch"
            | "save_team"
            | "list_teams"
            | "get_team"
            | "delete_team" => self.execute_background_agent_runtime_tool(&params).await?,
            "list_traces" => {
                let defaults = self.load_api_defaults().await?;
                let limit = params
                    .limit
                    .unwrap_or(defaults.background_trace_list_limit)
                    .max(1);
                let offset = params.offset.unwrap_or(0);
                let include_stats = params.include_stats.unwrap_or(false)
                    || params
                        .action
                        .as_deref()
                        .map(|action| action.trim().eq_ignore_ascii_case("stats"))
                        .unwrap_or(false);
                let category = Self::parse_trace_category(params.category)?;
                let source_filter = Self::normalize_optional_filter(params.source);
                Self::validate_trace_time_range(params.from_time_ms, params.to_time_ms)?;
                let status_filter = Self::parse_task_status(params.status)?;
                let query_task_id = params.task_id.clone();
                let query_id = params.id.clone();

                let task_selector = query_task_id.clone().or(query_id.clone());
                let scoped_tasks: Vec<BackgroundAgent> = if let Some(task_id) = task_selector {
                    let task = self
                        .backend
                        .get_background_agent(&task_id)
                        .await
                        .map_err(|e| format!("Failed to get task: {}", e))?;
                    let session_id = if task.chat_session_id.trim().is_empty() {
                        task.id.clone()
                    } else {
                        task.chat_session_id.clone()
                    };
                    let agent_id = task.agent_id.clone();

                    if let Some(agent_filter) = params.agent_id.as_deref()
                        && !agent_id.is_empty()
                        && agent_filter != agent_id
                    {
                        Vec::new()
                    } else {
                        let mut task = task;
                        task.chat_session_id = session_id;
                        vec![task]
                    }
                } else {
                    let mut tasks = self
                        .backend
                        .list_tasks(status_filter)
                        .await
                        .map_err(|e| format!("Failed to list tasks: {}", e))?;
                    if let Some(agent_id) = params.agent_id.as_deref() {
                        tasks.retain(|task| task.agent_id == agent_id);
                    }
                    tasks
                };

                let trace_fetch_limit = offset.saturating_add(limit).max(limit);
                let mut filtered_traces = Vec::new();
                for task in &scoped_tasks {
                    let session_id = if task.chat_session_id.trim().is_empty() {
                        task.id.as_str()
                    } else {
                        task.chat_session_id.as_str()
                    };
                    let traces = self
                        .backend
                        .query_execution_traces(ExecutionTraceQuery {
                            task_id: Some(task.id.clone()),
                            session_id: Some(session_id.to_string()),
                            limit: Some(trace_fetch_limit),
                            ..ExecutionTraceQuery::default()
                        })
                        .await
                        .map_err(|e| format!("Failed to list traces: {}", e))?;
                    filtered_traces.extend(traces.into_iter().filter(|trace| {
                        Self::trace_matches_category(trace, category.as_deref())
                            && Self::trace_matches_source(trace, source_filter.as_deref())
                            && Self::trace_matches_time_range(
                                trace,
                                params.from_time_ms,
                                params.to_time_ms,
                            )
                    }));
                }

                filtered_traces
                    .sort_by(|a, b| b.timestamp.cmp(&a.timestamp).then_with(|| a.id.cmp(&b.id)));
                let total = filtered_traces.len();
                let events: Vec<ExecutionTraceEvent> = filtered_traces
                    .iter()
                    .skip(offset)
                    .take(limit)
                    .cloned()
                    .collect();

                if include_stats {
                    serde_json::json!({
                        "events": serde_json::to_value(events).map_err(|e| e.to_string())?,
                        "stats": Self::build_trace_stats(
                            &filtered_traces,
                            limit,
                            offset,
                            scoped_tasks.len(),
                            scoped_tasks.len(),
                            params.from_time_ms,
                            params.to_time_ms,
                        ),
                        "query": {
                            "task_id": query_task_id,
                            "id": query_id,
                            "agent_id": params.agent_id,
                            "category": category,
                            "source": source_filter,
                            "from_time_ms": params.from_time_ms,
                            "to_time_ms": params.to_time_ms,
                            "limit": limit,
                            "offset": offset,
                            "total": total,
                        },
                    })
                } else {
                    serde_json::to_value(events).map_err(|e| e.to_string())?
                }
            }
            "read_trace" => {
                let trace_id = params.trace_id.ok_or("trace_id is required")?;
                let defaults = self.load_api_defaults().await?;
                let limit = params
                    .line_limit
                    .unwrap_or(defaults.background_trace_line_limit)
                    .max(1);
                let parts: Vec<&str> = trace_id.splitn(2, ':').collect();
                let traces = if parts.len() == 2 {
                    let mut query = ExecutionTraceQuery {
                        limit: Some(limit),
                        ..ExecutionTraceQuery::default()
                    };
                    query.session_id = Some(parts[0].to_string());
                    query.turn_id = Some(parts[1].to_string());
                    self.backend
                        .query_execution_traces(query)
                        .await
                        .map_err(|e| format!("Failed to read trace: {}", e))?
                } else {
                    self.backend
                        .query_execution_run_traces(&trace_id, limit)
                        .await
                        .map_err(|e| format!("Failed to read trace: {}", e))?
                };
                serde_json::json!({
                    "trace_id": trace_id,
                    "total": traces.len(),
                    "events": serde_json::to_value(&traces).map_err(|e| e.to_string())?,
                })
            }
            _ => {
                return Err(format!(
                    "Unknown operation: {}. Supported: {}",
                    operation, MANAGE_BACKGROUND_AGENT_OPERATIONS_CSV
                ));
            }
        };

        serde_json::to_string_pretty(&value).map_err(|e| e.to_string())
    }

    pub(crate) async fn apply_background_agent_api_defaults(
        &self,
        operation: &str,
        mut params: ManageBackgroundAgentsParams,
    ) -> Result<ManageBackgroundAgentsParams, String> {
        if matches!(operation, "progress" | "list_messages") {
            let defaults = self.load_api_defaults().await?;
            if operation == "progress" && params.event_limit.is_none() {
                params.event_limit = Some(defaults.background_progress_event_limit);
            }
            if operation == "list_messages" && params.limit.is_none() {
                params.limit = Some(defaults.background_message_list_limit);
            }
        }
        Ok(params)
    }

    pub(crate) async fn execute_background_agent_runtime_tool(
        &self,
        params: &ManageBackgroundAgentsParams,
    ) -> Result<Value, String> {
        let tool_input = serde_json::to_value(params)
            .map_err(|e| format!("Failed to serialize params: {}", e))?;
        let tool_result = self
            .backend
            .execute_runtime_tool("manage_background_agents", tool_input)
            .await
            .map_err(|e| Self::wrap_backend_error("Failed to execute runtime tool", e))?;
        if !tool_result.success {
            return Err(tool_result
                .error
                .unwrap_or_else(|| "manage_background_agents tool failed".to_string()));
        }
        Ok(tool_result.result)
    }
}
