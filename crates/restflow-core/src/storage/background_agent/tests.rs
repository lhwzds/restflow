use super::*;
use tempfile::tempdir;

fn create_test_storage() -> BackgroundAgentStorage {
    let temp_dir = tempdir().unwrap();
    let db_path = temp_dir.path().join("test.db");
    let db = Arc::new(Database::create(db_path).unwrap());
    BackgroundAgentStorage::new(db).unwrap()
}

// ============== Short ID Resolution Tests ==============

#[test]
fn test_resolve_existing_task_id_exact_match() {
    let storage = create_test_storage();

    let task = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Test Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("test input".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    // Full ID should resolve to itself
    let resolved = storage.resolve_existing_task_id(&task.id).unwrap();
    assert_eq!(resolved, task.id);
}

#[test]
fn test_resolve_existing_task_id_typed_exact_match() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Typed Exact".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let resolved = storage.resolve_existing_task_id_typed(&task.id).unwrap();
    assert_eq!(resolved, task.id);
}

#[test]
fn test_resolve_existing_task_id_unique_prefix() {
    let storage = create_test_storage();

    let task = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Test Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("test input".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    // 8-char prefix should resolve to full ID
    let prefix = &task.id[..8];
    let resolved = storage.resolve_existing_task_id(prefix).unwrap();
    assert_eq!(resolved, task.id);
}

#[test]
fn test_resolve_existing_task_id_typed_unique_prefix() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Typed Prefix".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let prefix = &task.id[..8];
    let resolved = storage.resolve_existing_task_id_typed(prefix).unwrap();
    assert_eq!(resolved, task.id);
}

#[test]
fn test_resolve_existing_task_id_unknown_prefix() {
    let storage = create_test_storage();

    let result = storage.resolve_existing_task_id("nonexist");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Task not found"));
}

#[test]
fn test_resolve_existing_task_id_typed_returns_not_found() {
    let storage = create_test_storage();

    let result = storage.resolve_existing_task_id_typed("nonexist");
    match result {
        Err(ResolveTaskIdError::NotFound(id)) => assert_eq!(id, "nonexist"),
        other => panic!("expected not found error, got {other:?}"),
    }
}

#[test]
fn test_resolve_existing_task_id_ambiguous_prefix() {
    let storage = create_test_storage();

    // Create multiple tasks
    let _task1 = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Task 1".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("test input".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let _task2 = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Task 2".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("test input".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    // Empty string should match all tasks (ambiguous)
    let result = storage.resolve_existing_task_id("");
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(
        err_msg.contains("ambiguous"),
        "Error should mention ambiguity"
    );
    assert!(
        err_msg.contains("Candidates"),
        "Error should list candidates"
    );
}

#[test]
fn test_resolve_existing_task_id_typed_returns_ambiguous() {
    let storage = create_test_storage();
    let raw_storage = storage.inner.clone();

    for id in ["shared-1", "shared-2"] {
        let task = BackgroundAgent::new(
            id.to_string(),
            format!("Task {id}"),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        );
        let raw = serde_json::to_vec(&task).unwrap();
        raw_storage
            .put_task_raw_with_status(id, task.status.as_str(), &raw)
            .unwrap();
    }

    let result = storage.resolve_existing_task_id_typed("shared");
    match result {
        Err(ResolveTaskIdError::Ambiguous { prefix, preview }) => {
            assert_eq!(prefix, "shared");
            assert!(preview.contains("shared-1"));
            assert!(preview.contains("shared-2"));
        }
        other => panic!("expected ambiguous error, got {other:?}"),
    }
}

#[test]
fn test_resolve_existing_task_id_typed_returns_internal_for_malformed_task_scan() {
    let storage = create_test_storage();
    storage
        .inner
        .put_task_raw_with_status("bad-task", "active", b"{bad-json")
        .unwrap();

    let result = storage.resolve_existing_task_id_typed("missing-prefix");
    match result {
        Err(ResolveTaskIdError::Internal(err)) => {
            assert!(err.to_string().contains("key must be a string"));
        }
        other => panic!("expected internal error, got {other:?}"),
    }
}

#[test]
fn test_resolve_existing_task_id_exact_priority_over_prefix() {
    let storage = create_test_storage();

    let task = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Test Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("test input".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    // Even if there's a prefix collision, exact match should win
    // (This is already the case because we check exact first)
    let resolved = storage.resolve_existing_task_id(&task.id).unwrap();
    assert_eq!(resolved, task.id);
}

// ============== Original Tests ==============

#[test]
fn test_create_and_get_task() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Test Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Interval {
                interval_ms: 3600000,
                start_at: None,
            },
        )
        .unwrap();

    assert!(!task.id.is_empty());
    assert_eq!(task.name, "Test Task");
    assert_eq!(task.agent_id, "agent-001");
    assert_eq!(task.status, BackgroundAgentStatus::Active);

    let retrieved = storage.get_task(&task.id).unwrap();
    assert!(retrieved.is_some());
    assert_eq!(retrieved.unwrap().name, "Test Task");
}

#[test]
fn test_list_tasks() {
    let storage = create_test_storage();

    storage
        .create_task(
            "Task 1".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    storage
        .create_task(
            "Task 2".to_string(),
            "agent-002".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let tasks = storage.list_tasks().unwrap();
    assert_eq!(tasks.len(), 2);
}

#[test]
fn test_get_task_returns_error_for_malformed_record() {
    let storage = create_test_storage();
    storage
        .inner
        .put_task_raw_with_status("bad-task", "active", b"{bad-json")
        .unwrap();

    let result = storage.get_task("bad-task");
    assert!(result.is_err());
}

#[test]
fn test_list_tasks_returns_error_when_any_record_is_malformed() {
    let storage = create_test_storage();
    storage
        .create_task(
            "Good Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    storage
        .inner
        .put_task_raw_with_status("bad-task", "active", b"{bad-json")
        .unwrap();

    let result = storage.list_tasks();
    assert!(result.is_err());
}

#[test]
fn test_list_tasks_by_status() {
    let storage = create_test_storage();

    let task1 = storage
        .create_task(
            "Active Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let task2 = storage
        .create_task(
            "Will be Paused".to_string(),
            "agent-002".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    storage.pause_task(&task2.id).unwrap();

    let active_tasks = storage
        .list_tasks_by_status(BackgroundAgentStatus::Active)
        .unwrap();
    let paused_tasks = storage
        .list_tasks_by_status(BackgroundAgentStatus::Paused)
        .unwrap();

    assert_eq!(active_tasks.len(), 1);
    assert_eq!(active_tasks[0].id, task1.id);
    assert_eq!(paused_tasks.len(), 1);
    assert_eq!(paused_tasks[0].id, task2.id);
}

#[test]
fn test_list_tasks_by_status_falls_back_to_full_scan_when_index_is_empty() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Fallback Target".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    // Corrupt index consistency intentionally:
    // persist payload as "paused" without updating status index.
    let mut paused_payload = storage.get_task(&task.id).unwrap().unwrap();
    paused_payload.status = BackgroundAgentStatus::Paused;
    paused_payload.updated_at += 1;
    let raw = serde_json::to_vec(&paused_payload).unwrap();
    storage.inner.put_task_raw(&task.id, &raw).unwrap();

    // Indexed query for paused should be empty, forcing fallback to full scan.
    let indexed_paused = storage
        .inner
        .list_tasks_by_status_indexed("paused")
        .unwrap();
    assert!(indexed_paused.is_empty());

    let paused_tasks = storage
        .list_tasks_by_status(BackgroundAgentStatus::Paused)
        .unwrap();
    assert_eq!(paused_tasks.len(), 1);
    assert_eq!(paused_tasks[0].id, task.id);
}

#[test]
fn test_list_tasks_by_status_recovers_from_partial_index_drift() {
    let storage = create_test_storage();

    let missing_from_index = storage
        .create_task(
            "Missing Paused Index".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    let indexed_paused = storage
        .create_task(
            "Indexed Paused".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    // Keep payload status paused, but intentionally skip status-index update.
    let mut paused_payload = storage.get_task(&missing_from_index.id).unwrap().unwrap();
    paused_payload.status = BackgroundAgentStatus::Paused;
    paused_payload.updated_at += 1;
    let raw = serde_json::to_vec(&paused_payload).unwrap();
    storage
        .inner
        .put_task_raw(&missing_from_index.id, &raw)
        .unwrap();

    storage.pause_task(&indexed_paused.id).unwrap();
    let indexed_only = storage
        .inner
        .list_tasks_by_status_indexed("paused")
        .unwrap();
    assert_eq!(indexed_only.len(), 1);
    assert_eq!(indexed_only[0].0, indexed_paused.id);

    let paused_tasks = storage
        .list_tasks_by_status(BackgroundAgentStatus::Paused)
        .unwrap();
    let ids: std::collections::HashSet<_> =
        paused_tasks.iter().map(|task| task.id.clone()).collect();
    assert_eq!(paused_tasks.len(), 2);
    assert!(ids.contains(&missing_from_index.id));
    assert!(ids.contains(&indexed_paused.id));
}

#[test]
fn test_save_task_status_transition_keeps_status_queries_consistent() {
    let storage = create_test_storage();
    let created = storage
        .create_task(
            "Save Transition".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let mut updated = storage.get_task(&created.id).unwrap().unwrap();
    updated.pause();
    storage.save_task(&updated).unwrap();

    let active_tasks = storage
        .list_tasks_by_status(BackgroundAgentStatus::Active)
        .unwrap();
    let paused_tasks = storage
        .list_tasks_by_status(BackgroundAgentStatus::Paused)
        .unwrap();

    assert!(active_tasks.iter().all(|task| task.id != created.id));
    assert_eq!(paused_tasks.len(), 1);
    assert_eq!(paused_tasks[0].id, created.id);
}

#[test]
fn test_status_index_consistency_after_multiple_status_transitions() {
    let storage = create_test_storage();
    let first = storage
        .create_task(
            "Transition A".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    let second = storage
        .create_task(
            "Transition B".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let mut transitioning = storage.get_task(&first.id).unwrap().unwrap();
    transitioning.pause();
    storage.save_task(&transitioning).unwrap();

    let mut transitioning = storage.get_task(&first.id).unwrap().unwrap();
    transitioning.resume();
    storage.save_task(&transitioning).unwrap();

    let active_tasks = storage
        .list_tasks_by_status(BackgroundAgentStatus::Active)
        .unwrap();
    let paused_tasks = storage
        .list_tasks_by_status(BackgroundAgentStatus::Paused)
        .unwrap();
    assert_eq!(active_tasks.len(), 2);
    assert!(active_tasks.iter().any(|task| task.id == first.id));
    assert!(active_tasks.iter().any(|task| task.id == second.id));
    assert!(paused_tasks.iter().all(|task| task.id != first.id));

    let indexed_active = storage
        .inner
        .list_tasks_by_status_indexed("active")
        .unwrap();
    let indexed_paused = storage
        .inner
        .list_tasks_by_status_indexed("paused")
        .unwrap();
    assert_eq!(indexed_active.len(), 2);
    assert!(indexed_active.iter().any(|(id, _)| id == &first.id));
    assert!(indexed_active.iter().any(|(id, _)| id == &second.id));
    assert!(indexed_paused.iter().all(|(id, _)| id != &first.id));
}

#[test]
fn test_list_tasks_by_agent_id() {
    let storage = create_test_storage();

    let task1 = storage
        .create_task(
            "Agent One Active".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    let task2 = storage
        .create_task(
            "Agent One Paused".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    let _task3 = storage
        .create_task(
            "Agent Two Active".to_string(),
            "agent-002".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    storage.pause_task(&task2.id).unwrap();

    let mut tasks = storage.list_tasks_by_agent_id("agent-001").unwrap();
    tasks.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(tasks.len(), 2);
    assert_eq!(tasks[0].id, task1.id);
    assert_eq!(tasks[1].id, task2.id);
}

#[test]
fn test_list_active_tasks_by_agent_id() {
    let storage = create_test_storage();

    let active = storage
        .create_task(
            "Active".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    let paused = storage
        .create_task(
            "Paused".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    let completed = storage
        .create_task(
            "Completed".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Once {
                run_at: chrono::Utc::now().timestamp_millis(),
            },
        )
        .unwrap();
    let recurring_failed = storage
        .create_task(
            "Recurring Failed".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
        )
        .unwrap();
    let once_failed = storage
        .create_task(
            "Once Failed".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Once {
                run_at: chrono::Utc::now().timestamp_millis() - 1_000,
            },
        )
        .unwrap();

    storage.pause_task(&paused.id).unwrap();
    storage.start_task_execution(&completed.id).unwrap();
    storage
        .complete_task_execution(&completed.id, Some("done".to_string()), 100)
        .unwrap();
    storage.start_task_execution(&recurring_failed.id).unwrap();
    storage
        .fail_task_execution(&recurring_failed.id, "retry me".to_string(), 100)
        .unwrap();
    storage.start_task_execution(&once_failed.id).unwrap();
    storage
        .fail_task_execution(&once_failed.id, "terminal".to_string(), 100)
        .unwrap();

    let mut tasks = storage.list_active_tasks_by_agent_id("agent-001").unwrap();
    tasks.sort_by(|a, b| a.name.cmp(&b.name));

    assert_eq!(tasks.len(), 3);
    assert_eq!(tasks[0].id, active.id);
    assert_eq!(tasks[1].id, paused.id);
    assert_eq!(tasks[2].id, recurring_failed.id);
}

#[test]
fn test_cleanup_old_tasks_keeps_non_terminal() {
    let storage = create_test_storage();
    let now = chrono::Utc::now().timestamp_millis();

    let terminal = storage
        .create_task(
            "Terminal Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Once {
                run_at: now - 10_000,
            },
        )
        .unwrap();
    storage
        .fail_task_execution(&terminal.id, "failed".to_string(), 1)
        .unwrap();
    let mut terminal_updated = storage.get_task(&terminal.id).unwrap().unwrap();
    terminal_updated.updated_at = now - (10 * 24 * 60 * 60 * 1000);
    storage.update_task(&terminal_updated).unwrap();

    let mut active = storage
        .create_task(
            "Active Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();
    active.updated_at = now - (30 * 24 * 60 * 60 * 1000);
    storage.update_task(&active).unwrap();

    let recurring_failed = storage
        .create_task(
            "Recurring Failed".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
        )
        .unwrap();
    storage
        .fail_task_execution(&recurring_failed.id, "retryable".to_string(), 1)
        .unwrap();
    let mut recurring_failed_updated = storage.get_task(&recurring_failed.id).unwrap().unwrap();
    recurring_failed_updated.updated_at = now - (10 * 24 * 60 * 60 * 1000);
    storage.update_task(&recurring_failed_updated).unwrap();

    let cutoff = now - (7 * 24 * 60 * 60 * 1000);
    let deleted = storage.cleanup_old_tasks(cutoff).unwrap();
    assert_eq!(deleted, 1);
    assert!(storage.get_task(&terminal.id).unwrap().is_none());
    assert!(storage.get_task(&active.id).unwrap().is_some());
    assert!(storage.get_task(&recurring_failed.id).unwrap().is_some());
}

#[test]
fn test_delete_task() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "To Delete".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    // Add some events
    let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Started);
    storage.add_event(&event).unwrap();
    let bg_message = storage
        .send_background_agent_message(
            &task.id,
            "queued message".to_string(),
            BackgroundMessageSource::User,
        )
        .unwrap();
    assert_eq!(bg_message.status, BackgroundMessageStatus::Queued);

    // Delete the task
    let deleted = storage.delete_task(&task.id).unwrap();
    assert!(deleted);

    // Task should be gone
    let retrieved = storage.get_task(&task.id).unwrap();
    assert!(retrieved.is_none());

    // Events should also be gone
    let events = storage.list_events_for_task(&task.id).unwrap();
    assert!(events.is_empty());

    // Background messages should also be gone
    let messages = storage
        .list_background_agent_messages(&task.id, 10)
        .unwrap();
    assert!(messages.is_empty());
}

#[test]
fn test_delete_task_archives_owned_chat_session() {
    let storage = create_test_storage();
    let task = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Archive On Delete".to_string(),
            agent_id: "agent-archive".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("archive me".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    assert!(task.owns_chat_session);
    let session_before = storage
        .chat_sessions()
        .get(&task.chat_session_id)
        .unwrap()
        .unwrap();
    assert!(session_before.archived_at.is_none());

    let deleted = storage.delete_task(&task.id).unwrap();
    assert!(deleted);
    let session_after = storage
        .chat_sessions()
        .get(&task.chat_session_id)
        .unwrap()
        .unwrap();
    assert!(session_after.archived_at.is_some());
}

#[test]
fn test_delete_task_does_not_archive_non_owned_chat_session() {
    let storage = create_test_storage();
    let shared_session = ChatSession::new("agent-shared".to_string(), "gpt-5".to_string());
    let shared_session_id = shared_session.id.clone();
    storage.chat_sessions().create(&shared_session).unwrap();

    let task = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "External Session".to_string(),
            agent_id: "agent-shared".to_string(),
            chat_session_id: Some(shared_session_id.clone()),
            description: None,
            input: Some("keep session".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    assert!(!task.owns_chat_session);
    let deleted = storage.delete_task(&task.id).unwrap();
    assert!(deleted);
    let session_after = storage
        .chat_sessions()
        .get(&shared_session_id)
        .unwrap()
        .unwrap();
    assert!(session_after.archived_at.is_none());
}

#[test]
fn test_create_background_agent_rejects_reused_chat_session_binding() {
    let storage = create_test_storage();
    let owner_task = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Owner".to_string(),
            agent_id: "agent-owner".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("owner".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let result = storage.create_background_agent(BackgroundAgentSpec {
        name: "Reuser".to_string(),
        agent_id: "agent-owner".to_string(),
        chat_session_id: Some(owner_task.chat_session_id.clone()),
        description: None,
        input: Some("reuse".to_string()),
        input_template: None,
        schedule: BackgroundAgentSchedule::default(),
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: Vec::new(),
        continuation: None,
    });

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("already bound to background task"));
}

#[test]
fn test_pause_and_resume_task() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Test Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    // Pause the task
    let paused = storage.pause_task(&task.id).unwrap();
    assert_eq!(paused.status, BackgroundAgentStatus::Paused);

    // Resume the task
    let resumed = storage.resume_task(&task.id).unwrap();
    assert_eq!(resumed.status, BackgroundAgentStatus::Active);

    // Check events were recorded
    let events = storage.list_events_for_task(&task.id).unwrap();
    let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
    assert!(event_types.contains(&&BackgroundAgentEventType::Paused));
    assert!(event_types.contains(&&BackgroundAgentEventType::Resumed));
}

#[test]
fn test_task_execution_lifecycle() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Test Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    // Start execution
    let running = storage.start_task_execution(&task.id).unwrap();
    assert_eq!(running.status, BackgroundAgentStatus::Running);
    assert!(running.last_run_at.is_some());

    // Complete execution
    let completed = storage
        .complete_task_execution(&task.id, Some("Success output".to_string()), 1500)
        .unwrap();
    assert_eq!(completed.status, BackgroundAgentStatus::Active);
    assert_eq!(completed.success_count, 1);

    // Check events
    let events = storage.list_events_for_task(&task.id).unwrap();
    let event_types: Vec<_> = events.iter().map(|e| &e.event_type).collect();
    assert!(event_types.contains(&&BackgroundAgentEventType::Started));
    assert!(event_types.contains(&&BackgroundAgentEventType::Completed));
}

#[test]
fn test_start_task_execution_emits_started_event_once() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Test Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let running = storage.start_task_execution(&task.id).unwrap();
    assert_eq!(running.status, BackgroundAgentStatus::Running);

    let err = storage
        .start_task_execution(&task.id)
        .unwrap_err()
        .to_string();
    assert!(err.contains("cannot start from status"));

    let events = storage.list_events_for_task(&task.id).unwrap();
    let started_count = events
        .iter()
        .filter(|event| event.event_type == BackgroundAgentEventType::Started)
        .count();
    assert_eq!(started_count, 1);
}

#[test]
fn test_task_execution_failure() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Test Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    // Start and fail execution
    storage.start_task_execution(&task.id).unwrap();
    let failed = storage
        .fail_task_execution(&task.id, "Test error".to_string(), 500)
        .unwrap();

    assert_eq!(failed.status, BackgroundAgentStatus::Failed);
    assert_eq!(failed.failure_count, 1);
    assert_eq!(failed.last_error, Some("Test error".to_string()));

    // Check events
    let events = storage.list_events_for_task(&task.id).unwrap();
    let failed_events: Vec<_> = events
        .iter()
        .filter(|e| e.event_type == BackgroundAgentEventType::Failed)
        .collect();
    assert_eq!(failed_events.len(), 1);
    assert_eq!(failed_events[0].message, Some("Test error".to_string()));
}

#[test]
fn test_start_task_execution_allows_retryable_failed_task() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Retryable Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
        )
        .unwrap();

    storage.start_task_execution(&task.id).unwrap();
    let failed = storage
        .fail_task_execution(&task.id, "retry me".to_string(), 500)
        .unwrap();
    assert_eq!(failed.status, BackgroundAgentStatus::Failed);
    assert!(failed.next_run_at.is_some());

    let restarted = storage.start_task_execution(&task.id).unwrap();
    assert_eq!(restarted.status, BackgroundAgentStatus::Running);
}

#[test]
fn test_list_recent_events() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Test Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    // Add multiple events
    for i in 0..5 {
        let event = BackgroundAgentEvent::new(task.id.clone(), BackgroundAgentEventType::Started)
            .with_message(format!("Event {}", i));
        storage.add_event(&event).unwrap();
    }

    let recent = storage.list_recent_events_for_task(&task.id, 3).unwrap();
    assert_eq!(recent.len(), 3);
}

#[test]
fn test_notification_events() {
    let storage = create_test_storage();

    let task = storage
        .create_task(
            "Test Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    // Record notification sent
    storage
        .record_notification_sent(&task.id, "Notification delivered".to_string())
        .unwrap();

    // Record notification failure
    storage
        .record_notification_failed(&task.id, "Network error".to_string())
        .unwrap();

    let events = storage.list_events_for_task(&task.id).unwrap();
    let notification_events: Vec<_> = events
        .iter()
        .filter(|e| {
            e.event_type == BackgroundAgentEventType::NotificationSent
                || e.event_type == BackgroundAgentEventType::NotificationFailed
        })
        .collect();

    assert_eq!(notification_events.len(), 2);
}

#[test]
fn test_list_runnable_tasks() {
    let storage = create_test_storage();

    // Create a task with a past run time
    let past_time = chrono::Utc::now().timestamp_millis() - 10000;
    let task1 = storage
        .create_task(
            "Ready Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Once { run_at: past_time },
        )
        .unwrap();

    // Manually fix the next_run_at to be in the past
    let mut task1_updated = task1;
    task1_updated.next_run_at = Some(past_time);
    storage.update_task(&task1_updated).unwrap();

    // Create a task with a future run time
    let future_time = chrono::Utc::now().timestamp_millis() + 3600000;
    storage
        .create_task(
            "Future Task".to_string(),
            "agent-002".to_string(),
            BackgroundAgentSchedule::Once {
                run_at: future_time,
            },
        )
        .unwrap();

    let current_time = chrono::Utc::now().timestamp_millis();
    let runnable = storage.list_runnable_tasks(current_time).unwrap();

    assert_eq!(runnable.len(), 1);
    assert_eq!(runnable[0].name, "Ready Task");
}

#[test]
fn test_list_runnable_tasks_repairs_missing_next_run_for_cron() {
    let storage = create_test_storage();

    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Cron Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("hello".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::Cron {
                expression: "* * * * *".to_string(),
                timezone: None,
            },
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    // Simulate legacy data where next_run_at was not computed.
    let mut broken = created.clone();
    broken.next_run_at = None;
    storage.update_task(&broken).unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    let _ = storage.list_runnable_tasks(now).unwrap();

    let repaired = storage.get_task(&created.id).unwrap().unwrap();
    assert!(repaired.next_run_at.is_some());
}

#[test]
fn test_list_runnable_tasks_repairs_stale_next_run() {
    let storage = create_test_storage();

    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Stale Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("hello".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::Interval {
                interval_ms: 900_000,
                start_at: None,
            },
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    // Simulate stale state: next_run_at is before last_run_at.
    // This happens when the daemon restarts mid-execution and
    // the completion handler doesn't persist the updated schedule.
    let now = chrono::Utc::now().timestamp_millis();
    let mut broken = created.clone();
    broken.next_run_at = Some(now - 3_600_000); // 1 hour ago
    broken.last_run_at = Some(now - 1_800_000); // 30 min ago (more recent)
    storage.update_task(&broken).unwrap();

    // Verify the stale condition
    let before = storage.get_task(&created.id).unwrap().unwrap();
    assert!(before.next_run_at.unwrap() < before.last_run_at.unwrap());

    // list_runnable_tasks should repair this
    let _ = storage.list_runnable_tasks(now).unwrap();

    let repaired = storage.get_task(&created.id).unwrap().unwrap();
    assert!(
        repaired.next_run_at.unwrap() > now,
        "next_run_at should be in the future after repair"
    );
}

#[test]
fn test_repair_runnable_task_does_not_overwrite_paused_status_from_stale_snapshot() {
    let storage = create_test_storage();

    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Pause Race Guard".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("hello".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::Interval {
                interval_ms: 900_000,
                start_at: None,
            },
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let now = chrono::Utc::now().timestamp_millis();
    let mut stale_active_snapshot = storage.get_task(&created.id).unwrap().unwrap();
    stale_active_snapshot.next_run_at = Some(now - 3_600_000);
    stale_active_snapshot.last_run_at = Some(now - 1_800_000);
    storage.update_task(&stale_active_snapshot).unwrap();
    let stale_active_snapshot = storage.get_task(&created.id).unwrap().unwrap();

    storage.pause_task(&created.id).unwrap();
    let paused_before_repair = storage.get_task(&created.id).unwrap().unwrap();
    assert_eq!(paused_before_repair.status, BackgroundAgentStatus::Paused);

    let repaired = storage
        .repair_runnable_task_if_needed(stale_active_snapshot)
        .unwrap();
    assert!(repaired.is_none());

    let after = storage.get_task(&created.id).unwrap().unwrap();
    assert_eq!(after.status, BackgroundAgentStatus::Paused);
    assert_eq!(after.next_run_at, paused_before_repair.next_run_at);
    assert_eq!(after.last_run_at, paused_before_repair.last_run_at);
}

#[test]
fn test_list_runnable_tasks_skips_task_when_repair_persist_fails() {
    let storage = create_test_storage();

    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Repair Failure".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("hello".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::Interval {
                interval_ms: 3_600_000,
                start_at: None,
            },
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    // Create a stale schedule state that requires repair and is runnable now.
    let now = chrono::Utc::now().timestamp_millis();
    let mut broken = storage.get_task(&created.id).unwrap().unwrap();
    broken.next_run_at = Some(now - 5_000);
    broken.last_run_at = Some(now - 1_000);
    storage.update_task(&broken).unwrap();
    assert!(broken.should_run(now));

    // Create another runnable task to prove list_runnable_tasks still returns other tasks.
    let ready = storage
        .create_task(
            "Control Runnable".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
        )
        .unwrap();
    let mut ready_task = storage.get_task(&ready.id).unwrap().unwrap();
    ready_task.next_run_at = Some(now - 10_000);
    storage.update_task(&ready_task).unwrap();
    assert!(ready_task.should_run(now));

    // Inject a conflicting task with same chat_session_id by bypassing uniqueness checks.
    // This makes update_task fail during repair persistence.
    let mut conflicting = broken.clone();
    conflicting.id = format!("conflict-{}", Uuid::new_v4());
    conflicting.status = BackgroundAgentStatus::Paused;
    let conflicting_raw = serde_json::to_vec(&conflicting).unwrap();
    storage
        .inner
        .put_task_raw(&conflicting.id, &conflicting_raw)
        .unwrap();

    let runnable = storage.list_runnable_tasks(now).unwrap();
    assert!(runnable.iter().all(|task| task.id != created.id));
    assert!(runnable.iter().any(|task| task.id == ready.id));

    let after = storage.get_task(&created.id).unwrap().unwrap();
    assert_eq!(after.next_run_at, broken.next_run_at);
    assert_eq!(after.last_run_at, broken.last_run_at);
}

#[test]
fn test_get_nonexistent_task() {
    let storage = create_test_storage();

    let result = storage.get_task("nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_update_nonexistent_task_returns_error() {
    use crate::models::TaskSchedule;
    let storage = create_test_storage();
    let task = BackgroundAgent::new(
        "nonexistent".to_string(),
        "Ghost".to_string(),
        "agent-000".to_string(),
        TaskSchedule::Once {
            run_at: chrono::Utc::now().timestamp_millis() + 60_000,
        },
    );
    let result = storage.update_task(&task);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_pause_nonexistent_task() {
    let storage = create_test_storage();

    let result = storage.pause_task("nonexistent");
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("not found"));
}

#[test]
fn test_background_agent_lifecycle() {
    let storage = create_test_storage();

    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "BG Agent".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: Some("Background agent".to_string()),
            input: Some("Run checks".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::Interval {
                interval_ms: 60_000,
                start_at: None,
            },
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();
    assert_eq!(created.name, "BG Agent");

    let updated = storage
        .update_background_agent(
            &created.id,
            BackgroundAgentPatch {
                name: Some("BG Agent Updated".to_string()),
                ..Default::default()
            },
        )
        .unwrap();
    assert_eq!(updated.name, "BG Agent Updated");

    let paused = storage
        .control_background_agent(&created.id, BackgroundAgentControlAction::Pause)
        .unwrap();
    assert_eq!(paused.status, BackgroundAgentStatus::Paused);

    let resumed = storage
        .control_background_agent(&created.id, BackgroundAgentControlAction::Resume)
        .unwrap();
    assert_eq!(resumed.status, BackgroundAgentStatus::Active);

    let run_now = storage
        .control_background_agent(&created.id, BackgroundAgentControlAction::RunNow)
        .unwrap();
    assert_eq!(run_now.status, BackgroundAgentStatus::Active);
    assert!(run_now.next_run_at.is_some());

    let started = storage
        .control_background_agent(&created.id, BackgroundAgentControlAction::Start)
        .unwrap();
    assert_eq!(started.status, BackgroundAgentStatus::Active);
    assert!(started.next_run_at.is_some());

    let stopped = storage
        .control_background_agent(&created.id, BackgroundAgentControlAction::Stop)
        .unwrap();
    assert_eq!(stopped.status, BackgroundAgentStatus::Interrupted);
}

#[test]
fn test_background_message_queue_and_progress() {
    let storage = create_test_storage();
    let task = storage
        .create_task(
            "Message Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let queued = storage
        .send_background_agent_message(
            &task.id,
            "Please also verify logs".to_string(),
            BackgroundMessageSource::User,
        )
        .unwrap();
    assert_eq!(queued.status, BackgroundMessageStatus::Queued);

    let pending = storage
        .list_pending_background_messages(&task.id, 10)
        .unwrap();
    assert_eq!(pending.len(), 1);

    let delivered = storage
        .mark_background_message_delivered(&queued.id)
        .unwrap()
        .unwrap();
    assert_eq!(delivered.status, BackgroundMessageStatus::Delivered);

    let consumed = storage
        .mark_background_message_consumed(&queued.id)
        .unwrap()
        .unwrap();
    assert_eq!(consumed.status, BackgroundMessageStatus::Consumed);

    let progress = storage.get_background_agent_progress(&task.id, 5).unwrap();
    assert_eq!(progress.background_agent_id, task.id);
    assert_eq!(progress.pending_message_count, 0);
}

#[test]
fn test_log_background_agent_reply_is_not_queued() {
    let storage = create_test_storage();
    let task = storage
        .create_task(
            "Reply Task".to_string(),
            "agent-001".to_string(),
            BackgroundAgentSchedule::default(),
        )
        .unwrap();

    let reply = storage
        .log_background_agent_reply(&task.id, "ack".to_string())
        .unwrap();
    assert_eq!(reply.source, BackgroundMessageSource::Agent);
    assert_eq!(reply.status, BackgroundMessageStatus::Consumed);
    assert!(reply.delivered_at.is_some());
    assert!(reply.consumed_at.is_some());

    let pending = storage
        .list_pending_background_messages(&task.id, 10)
        .unwrap();
    assert!(pending.is_empty());
}

#[test]
fn test_create_background_agent_with_template_and_memory_scope() {
    use crate::models::{MemoryConfig, MemoryScope};

    let storage = create_test_storage();
    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Templated Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("fallback".to_string()),
            input_template: Some("Run task {{task.id}}".to_string()),
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: Some(MemoryConfig {
                max_messages: 120,
                enable_file_memory: true,
                persist_on_complete: true,
                memory_scope: MemoryScope::PerBackgroundAgent,
                enable_compaction: true,
                compaction_threshold_ratio: 0.80,
                max_summary_tokens: 2_000,
            }),
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    assert_eq!(
        created.input_template.as_deref(),
        Some("Run task {{task.id}}")
    );
    assert_eq!(created.memory.memory_scope, MemoryScope::PerBackgroundAgent);
}

#[test]
fn test_create_background_agent_auto_creates_bound_chat_session() {
    let storage = create_test_storage();
    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Bound Session Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Run with auto session".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    assert!(!created.chat_session_id.trim().is_empty());
    let session = storage
        .chat_sessions
        .get(&created.chat_session_id)
        .unwrap()
        .unwrap();
    assert_eq!(session.agent_id, "agent-001");
    assert!(session.name.contains("Bound Session Task"));
}

#[test]
fn test_create_background_agent_rejects_chat_session_bound_to_other_agent() {
    let storage = create_test_storage();
    let foreign_session = ChatSession::new(
        "agent-002".to_string(),
        ModelId::Gpt5.as_serialized_str().to_string(),
    );
    storage.chat_sessions.create(&foreign_session).unwrap();

    let result = storage.create_background_agent(BackgroundAgentSpec {
        name: "Reject Foreign Session".to_string(),
        agent_id: "agent-001".to_string(),
        chat_session_id: Some(foreign_session.id.clone()),
        description: None,
        input: Some("Run".to_string()),
        input_template: None,
        schedule: BackgroundAgentSchedule::default(),
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: Vec::new(),
        continuation: None,
    });

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("is bound to agent"));
    assert!(err.contains("agent-002"));
}

#[test]
fn test_update_background_agent_agent_change_rebinds_chat_session() {
    let storage = create_test_storage();
    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Rebind Session Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Run".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();
    let original_session_id = created.chat_session_id.clone();

    let updated = storage
        .update_background_agent(
            &created.id,
            BackgroundAgentPatch {
                agent_id: Some("agent-002".to_string()),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(updated.agent_id, "agent-002");
    assert_ne!(updated.chat_session_id, original_session_id);

    let rebound_session = storage
        .chat_sessions
        .get(&updated.chat_session_id)
        .unwrap()
        .unwrap();
    assert_eq!(rebound_session.agent_id, "agent-002");
}

#[test]
fn test_update_background_agent_rejects_reused_chat_session_binding() {
    let storage = create_test_storage();
    let owner = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Owner".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Owner input".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let other = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Other".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Other input".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let result = storage.update_background_agent(
        &other.id,
        BackgroundAgentPatch {
            chat_session_id: Some(owner.chat_session_id.clone()),
            ..Default::default()
        },
    );
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("already bound to background task"));
}

#[test]
fn test_update_background_agent_updates_template_and_memory_scope() {
    use crate::models::{MemoryConfig, MemoryScope};

    let storage = create_test_storage();
    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Updatable Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Fallback task input".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let updated = storage
        .update_background_agent(
            &created.id,
            BackgroundAgentPatch {
                input_template: Some("Template {{task.name}}".to_string()),
                memory: Some(MemoryConfig {
                    max_messages: 80,
                    enable_file_memory: false,
                    persist_on_complete: true,
                    memory_scope: MemoryScope::PerBackgroundAgent,
                    enable_compaction: true,
                    compaction_threshold_ratio: 0.80,
                    max_summary_tokens: 2_000,
                }),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(
        updated.input_template.as_deref(),
        Some("Template {{task.name}}")
    );
    assert_eq!(updated.memory.memory_scope, MemoryScope::PerBackgroundAgent);
}

#[test]
fn test_create_background_agent_rejects_timeout_below_minimum() {
    let storage = create_test_storage();
    let result = storage.create_background_agent(BackgroundAgentSpec {
        name: "Too Fast Timeout".to_string(),
        agent_id: "agent-001".to_string(),
        chat_session_id: None,
        description: None,
        input: Some("Run timeout validation".to_string()),
        input_template: None,
        schedule: BackgroundAgentSchedule::default(),
        notification: None,
        execution_mode: None,
        timeout_secs: Some(5),
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: Vec::new(),
        continuation: None,
    });

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("timeout_secs must be at least")
    );
}

#[test]
fn test_update_background_agent_updates_timeout_secs() {
    let storage = create_test_storage();
    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Timeout Update Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Run timeout update".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let updated = storage
        .update_background_agent(
            &created.id,
            BackgroundAgentPatch {
                timeout_secs: Some(900),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(updated.timeout_secs, Some(900));
}

#[test]
fn test_background_agent_resource_limits_roundtrip() {
    use crate::models::ResourceLimits;

    let storage = create_test_storage();
    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Resource Limits Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Run resource limit roundtrip".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: Some(ResourceLimits {
                max_tool_calls: 12,
                max_duration_secs: 90,
                max_output_bytes: 2048,
                max_cost_usd: Some(1.25),
            }),
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    assert_eq!(created.resource_limits.max_tool_calls, 12);
    assert_eq!(created.resource_limits.max_duration_secs, 90);
    assert_eq!(created.resource_limits.max_output_bytes, 2048);
    assert_eq!(created.resource_limits.max_cost_usd, Some(1.25));

    let updated = storage
        .update_background_agent(
            &created.id,
            BackgroundAgentPatch {
                resource_limits: Some(ResourceLimits {
                    max_tool_calls: 34,
                    max_duration_secs: 120,
                    max_output_bytes: 4096,
                    max_cost_usd: Some(2.5),
                }),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(updated.resource_limits.max_tool_calls, 34);
    assert_eq!(updated.resource_limits.max_duration_secs, 120);
    assert_eq!(updated.resource_limits.max_output_bytes, 4096);
    assert_eq!(updated.resource_limits.max_cost_usd, Some(2.5));
}

#[test]
fn test_background_agent_continuation_roundtrip() {
    use crate::models::ContinuationConfig;

    let storage = create_test_storage();
    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Continuation Task".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Run continuation roundtrip".to_string()),
            input_template: None,
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: Some(ContinuationConfig {
                enabled: true,
                segment_iterations: 40,
                max_total_iterations: 800,
                max_total_cost_usd: Some(4.5),
                inter_segment_pause_ms: 250,
            }),
        })
        .unwrap();

    assert!(created.continuation.enabled);
    assert_eq!(created.continuation.segment_iterations, 40);
    assert_eq!(created.continuation.max_total_iterations, 800);
    assert_eq!(created.continuation.max_total_cost_usd, Some(4.5));
    assert_eq!(created.continuation.inter_segment_pause_ms, 250);
    assert_eq!(created.continuation_total_iterations, 0);
    assert_eq!(created.continuation_segments_completed, 0);

    let mut advanced = created.clone();
    advanced.continuation_total_iterations = 120;
    advanced.continuation_segments_completed = 3;
    storage.update_task(&advanced).unwrap();

    let updated = storage
        .update_background_agent(
            &created.id,
            BackgroundAgentPatch {
                continuation: Some(ContinuationConfig {
                    enabled: true,
                    segment_iterations: 60,
                    max_total_iterations: 1_200,
                    max_total_cost_usd: Some(6.0),
                    inter_segment_pause_ms: 500,
                }),
                ..Default::default()
            },
        )
        .unwrap();

    assert_eq!(updated.continuation.segment_iterations, 60);
    assert_eq!(updated.continuation.max_total_iterations, 1_200);
    assert_eq!(updated.continuation.max_total_cost_usd, Some(6.0));
    assert_eq!(updated.continuation.inter_segment_pause_ms, 500);
    assert_eq!(updated.continuation_total_iterations, 0);
    assert_eq!(updated.continuation_segments_completed, 0);
}

#[test]
fn test_create_background_agent_rejects_missing_input_and_template() {
    let storage = create_test_storage();
    let result = storage.create_background_agent(BackgroundAgentSpec {
        name: "Missing Input".to_string(),
        agent_id: "agent-001".to_string(),
        chat_session_id: None,
        description: None,
        input: None,
        input_template: None,
        schedule: BackgroundAgentSchedule::default(),
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: Vec::new(),
        continuation: None,
    });

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("requires non-empty input or input_template")
    );
}

#[test]
fn test_update_background_agent_rejects_empty_input_and_template() {
    let storage = create_test_storage();
    let created = storage
        .create_background_agent(BackgroundAgentSpec {
            name: "Mutable Input".to_string(),
            agent_id: "agent-001".to_string(),
            chat_session_id: None,
            description: None,
            input: Some("Initial input".to_string()),
            input_template: Some("Template {{task.name}}".to_string()),
            schedule: BackgroundAgentSchedule::default(),
            notification: None,
            execution_mode: None,
            timeout_secs: None,
            memory: None,
            durability_mode: None,
            resource_limits: None,
            prerequisites: Vec::new(),
            continuation: None,
        })
        .unwrap();

    let result = storage.update_background_agent(
        &created.id,
        BackgroundAgentPatch {
            input: Some("".to_string()),
            input_template: Some("   ".to_string()),
            ..Default::default()
        },
    );

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("requires non-empty input or input_template")
    );
}

#[test]
fn test_create_background_agent_allows_empty_template_render_when_fallback_input_exists() {
    let storage = create_test_storage();
    let result = storage.create_background_agent(BackgroundAgentSpec {
        name: "Fallback Input".to_string(),
        agent_id: "agent-001".to_string(),
        chat_session_id: None,
        description: None,
        input: Some("Use fallback".to_string()),
        input_template: Some("{{input}}".to_string()),
        schedule: BackgroundAgentSchedule::default(),
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: Vec::new(),
        continuation: None,
    });

    assert!(result.is_ok());
}

#[test]
fn test_create_background_agent_rejects_template_that_renders_empty_without_fallback() {
    let storage = create_test_storage();
    let result = storage.create_background_agent(BackgroundAgentSpec {
        name: "Empty Template".to_string(),
        agent_id: "agent-001".to_string(),
        chat_session_id: None,
        description: None,
        input: None,
        input_template: Some("{{input}}".to_string()),
        schedule: BackgroundAgentSchedule::default(),
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: Vec::new(),
        continuation: None,
    });

    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("requires non-empty input or input_template")
    );
}

#[test]
fn test_create_background_agent_keeps_non_empty_template_compatibility() {
    let storage = create_test_storage();
    let result = storage.create_background_agent(BackgroundAgentSpec {
        name: "Template Compatibility".to_string(),
        agent_id: "agent-001".to_string(),
        chat_session_id: None,
        description: None,
        input: None,
        input_template: Some("Task {{task.name}}".to_string()),
        schedule: BackgroundAgentSchedule::default(),
        notification: None,
        execution_mode: None,
        timeout_secs: None,
        memory: None,
        durability_mode: None,
        resource_limits: None,
        prerequisites: Vec::new(),
        continuation: None,
    });

    assert!(result.is_ok());
}
