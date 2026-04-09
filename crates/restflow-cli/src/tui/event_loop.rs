use std::io;
use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};
use crossterm::event::{self, Event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use serde_json::json;
use tokio::sync::mpsc;

use super::daemon_client::TuiDaemonClient;
use super::keymap::{Action, map_event};
use super::render;
use super::state::{AppState, OverlayState, RunPickerItem, TranscriptKind};

use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::runtime::TaskStreamEvent;
use restflow_traits::{PendingTeamApproval, TeamState};

#[derive(Debug)]
pub enum AppEvent {
    Terminal(Event),
    StreamFrame(StreamFrame),
    SessionEvent(ChatSessionEvent),
    TaskEvent(TaskStreamEvent),
    RefreshCurrentSession,
    Error(String),
}

pub async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    client: TuiDaemonClient,
    mut state: AppState,
    initial_message: Option<String>,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let _input_handle = spawn_input_thread(tx.clone());
    let session_stream_handle = client.spawn_session_events(tx.clone());
    let mut selected_task_stream: Option<(String, tokio::task::JoinHandle<()>)> = None;

    refresh_caches(&client, &mut state).await?;
    if let Some(message) = initial_message {
        submit_message(&client, &mut state, message, tx.clone()).await?;
    }

    let mut tick = tokio::time::interval(Duration::from_secs(3));

    loop {
        terminal.draw(|frame| render::render(frame, &state))?;

        tokio::select! {
            _ = tick.tick() => {
                refresh_caches(&client, &mut state).await?;
            }
            maybe_event = rx.recv() => {
                let Some(event) = maybe_event else { break; };
                match event {
                    AppEvent::Terminal(event) => {
                        if handle_terminal_event(&client, terminal, &mut state, event, tx.clone()).await? {
                            break;
                        }
                    }
                    AppEvent::StreamFrame(frame) => state.apply_stream_frame(frame),
                    AppEvent::SessionEvent(event) => {
                        if state.current_session.as_ref().map(|session| session.id.as_str()) == Some(session_id_of(&event))
                            && let Ok(session) = client.get_session(session_id_of(&event)).await
                        {
                            state.set_current_session(session);
                        }
                        state.apply_session_event(event);
                        refresh_caches(&client, &mut state).await?;
                    }
                    AppEvent::TaskEvent(event) => {
                        state.apply_task_event(event);
                        refresh_caches(&client, &mut state).await?;
                    }
                    AppEvent::RefreshCurrentSession => {
                        if let Some(session_id) = state.current_session.as_ref().map(|session| session.id.clone())
                            && let Ok(session) = client.get_session(&session_id).await
                        {
                            state.set_current_session(session);
                        }
                        refresh_caches(&client, &mut state).await?;
                    }
                    AppEvent::Error(message) => {
                        state.status = message.clone();
                        state.push_transcript(TranscriptKind::Error, message);
                    }
                }
            }
        }

        sync_task_subscription(&client, &state, &tx, &mut selected_task_stream);
    }

    session_stream_handle.abort();
    if let Some((_, handle)) = selected_task_stream.take() {
        handle.abort();
    }

    Ok(())
}

fn spawn_input_thread(tx: mpsc::UnboundedSender<AppEvent>) -> thread::JoinHandle<()> {
    thread::spawn(move || loop {
        if let Ok(true) = event::poll(Duration::from_millis(100)) {
            match event::read() {
                Ok(event) => {
                    if tx.send(AppEvent::Terminal(event)).is_err() {
                        break;
                    }
                }
                Err(_) => break,
            }
        }
    })
}

async fn handle_terminal_event(
    client: &TuiDaemonClient,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    event: Event,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<bool> {
    match map_event(event) {
        Action::Quit => return Ok(true),
        Action::CloseOverlay => {
            if state.overlay.is_some() {
                state.clear_overlay();
            } else {
                return Ok(true);
            }
        }
        Action::OpenSessions => state.open_session_picker(),
        Action::OpenRuns => state.open_run_picker(),
        Action::OpenApprovals => state.open_approval_picker(),
        Action::OpenTeam => state.open_team_overlay(),
        Action::OpenHelp => state.open_help_overlay(),
        Action::Redraw => {
            terminal.clear()?;
            state.status = "Screen redrawn".to_string();
        }
        Action::NavUp => {
            if state.overlay.is_some() {
                state.move_overlay_selection(-1);
            } else if state.input.text.trim().is_empty() {
                state.history_previous();
            } else {
                state.scroll_transcript(-1);
            }
        }
        Action::NavDown => {
            if state.overlay.is_some() {
                state.move_overlay_selection(1);
            } else if state.history_cursor.is_some() {
                state.history_next();
            } else {
                state.scroll_transcript(1);
            }
        }
        Action::MoveLeft => {
            if matches!(state.overlay, Some(OverlayState::TeamView { .. })) {
                state.cycle_team_tab(false);
            } else {
                state.input.move_left();
            }
        }
        Action::MoveRight => {
            if matches!(state.overlay, Some(OverlayState::TeamView { .. })) {
                state.cycle_team_tab(true);
            } else {
                state.input.move_right();
            }
        }
        Action::ScrollUp => state.scroll_transcript(-10),
        Action::ScrollDown => state.scroll_transcript(10),
        Action::InputChar(ch) => {
            if state.overlay.is_none() {
                state.input.insert_char(ch);
            }
        }
        Action::InputBackspace => {
            if state.overlay.is_none() {
                state.input.backspace();
            }
        }
        Action::Newline => {
            if state.overlay.is_none() {
                state.input.insert_newline();
            }
        }
        Action::RejectSelected => {
            if matches!(state.overlay, Some(OverlayState::ApprovalPicker { .. })) {
                reject_selected_approval(client, state).await?;
            } else if state.overlay.is_none() {
                state.input.insert_char('r');
            }
        }
        Action::OverlaySelect => {
            if state.overlay.is_some() {
                activate_overlay_selection(client, state).await?;
            }
        }
        Action::Submit => {
            if state.overlay.is_some() {
                activate_overlay_selection(client, state).await?;
            } else {
                let input = state.input.take();
                if !input.trim().is_empty() {
                    state.push_history(input.clone());
                    if input.trim_start().starts_with('/') {
                        execute_slash_command(client, state, input).await?;
                    } else {
                        submit_message(client, state, input, tx).await?;
                    }
                }
            }
        }
        Action::Noop => {}
    }

    Ok(false)
}

async fn activate_overlay_selection(client: &TuiDaemonClient, state: &mut AppState) -> Result<()> {
    match state.overlay.clone() {
        Some(OverlayState::SessionPicker { .. }) => {
            if let Some(session_id) = state.selected_session_id().map(str::to_string) {
                let session = client.get_session(&session_id).await?;
                state.set_current_session(session);
                state.runs = client.list_runs_for_session(&session_id).await.unwrap_or_default();
                state.clear_overlay();
                state.status = format!("Opened session {session_id}");
            }
        }
        Some(OverlayState::RunPicker { .. }) => {
            if let Some(item) = state.selected_run_picker_item() {
                match item {
                    RunPickerItem::Task { id, .. } => {
                        state.set_current_task(id.clone());
                        state.runs = client.list_runs_for_task(&id).await.unwrap_or_default();
                        state.child_runs.clear();
                        state.clear_overlay();
                        state.status = format!("Opened task {id}");
                    }
                    RunPickerItem::Run { run_id, .. } => {
                        if let Ok(thread) = client.get_execution_run_thread(&run_id).await {
                            state.child_runs = client.list_child_runs(&run_id).await.unwrap_or_default();
                            if let Some(session_id) = thread.focus.session_id.as_deref()
                                && let Ok(session) = client.get_session(session_id).await
                            {
                                state.set_current_session(session);
                            }
                            state.set_current_run(run_id.clone(), thread);
                            state.clear_overlay();
                            state.status = format!("Opened run {run_id}");
                        }
                    }
                }
            }
        }
        Some(OverlayState::ApprovalPicker { .. }) => {
            approve_selected_approval(client, state).await?;
        }
        Some(OverlayState::TeamView { .. }) | Some(OverlayState::Help) | None => {}
    }
    Ok(())
}

async fn refresh_caches(client: &TuiDaemonClient, state: &mut AppState) -> Result<()> {
    state.sessions = client.list_sessions().await.unwrap_or_default();
    state.tasks = client.list_tasks().await.unwrap_or_default();

    if let Some(session_id) = state.current_session.as_ref().map(|session| session.id.clone()) {
        state.runs = client.list_runs_for_session(&session_id).await.unwrap_or_default();
    } else {
        state.runs.clear();
    }

    if let Some(team_run_id) = state.current_team_state.as_ref().map(|team| team.team_run_id.clone()) {
        let _ = load_team_state(client, state, &team_run_id).await;
    }

    Ok(())
}

fn sync_task_subscription(
    client: &TuiDaemonClient,
    state: &AppState,
    tx: &mpsc::UnboundedSender<AppEvent>,
    slot: &mut Option<(String, tokio::task::JoinHandle<()>)>,
) {
    let desired = state.current_task_id.clone();
    match (slot.as_ref().map(|(id, _)| id.clone()), desired) {
        (Some(current), Some(desired)) if current == desired => {}
        (current, Some(desired)) => {
            if current.is_some() && let Some((_, handle)) = slot.take() {
                handle.abort();
            }
            *slot = Some((desired.clone(), client.spawn_task_events(desired, tx.clone())));
        }
        (Some(_), None) => {
            if let Some((_, handle)) = slot.take() {
                handle.abort();
            }
        }
        (None, None) => {}
    }
}

async fn submit_message(
    client: &TuiDaemonClient,
    state: &mut AppState,
    message: String,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<()> {
    let session_id = match state.current_session.as_ref() {
        Some(session) => session.id.clone(),
        None => bail!("No active session available."),
    };
    state.push_transcript(TranscriptKind::User, message.clone());
    state.status = "Sending message...".to_string();
    client.spawn_chat_stream(session_id, message, tx);
    Ok(())
}

async fn execute_slash_command(
    client: &TuiDaemonClient,
    state: &mut AppState,
    raw: String,
) -> Result<()> {
    let mut parts = raw.split_whitespace();
    let command = parts.next().unwrap_or_default();
    match command {
        "/help" => state.open_help_overlay(),
        "/task" => {
            let action = parts.next().unwrap_or_default();
            let task_id = parts.next().unwrap_or_default();
            if task_id.is_empty() {
                bail!("/task requires an id");
            }
            let task = client.control_task(task_id, action).await?;
            state.status = format!("Task {} -> {:?}", task.id, task.status);
        }
        "/run" => {
            let action = parts.next().unwrap_or_default();
            let run_id = parts.next().unwrap_or_default();
            if action != "open" || run_id.is_empty() {
                bail!("Usage: /run open <run_id>");
            }
            if let Ok(thread) = client.get_execution_run_thread(run_id).await {
                state.child_runs = client.list_child_runs(run_id).await.unwrap_or_default();
                if let Some(session_id) = thread.focus.session_id.as_deref()
                    && let Ok(session) = client.get_session(session_id).await
                {
                    state.set_current_session(session);
                }
                state.set_current_run(run_id.to_string(), thread);
                state.status = format!("Opened run {run_id}");
            }
        }
        "/team" => {
            let action = parts.next().unwrap_or_default();
            match action {
                "state" => {
                    let team_run_id = parts.next().unwrap_or_default();
                    if team_run_id.is_empty() {
                        bail!("Usage: /team state <team_run_id>");
                    }
                    load_team_state(client, state, team_run_id).await?;
                    state.open_team_overlay();
                }
                "start" => {
                    let team = parts.next().unwrap_or_default();
                    if team.is_empty() {
                        bail!("Usage: /team start <saved_team>");
                    }
                    let output = client
                        .execute_runtime_tool(
                            "manage_teams",
                            json!({
                                "operation": "start_team",
                                "team": team,
                            }),
                        )
                        .await?;
                    if !output.success {
                        bail!(output.error.unwrap_or_else(|| "manage_teams failed".to_string()));
                    }
                    if let Ok(team_state) =
                        serde_json::from_value::<TeamState>(output.result["team"].clone())
                    {
                        let team_run_id = team_state.team_run_id.clone();
                        state.current_team_state = Some(team_state);
                        load_team_state(client, state, &team_run_id).await?;
                        state.open_team_overlay();
                        state.status = format!("Started team {team_run_id}");
                    }
                }
                _ => bail!("Unsupported /team action"),
            }
        }
        "/approve" => {
            let approval_id = parts.next().unwrap_or_default();
            approve_named_approval(client, state, approval_id).await?;
        }
        "/reject" => {
            let approval_id = parts.next().unwrap_or_default();
            let reason = parts.collect::<Vec<_>>().join(" ");
            reject_named_approval(client, state, approval_id, (!reason.is_empty()).then_some(reason)).await?;
        }
        _ => {
            state.status = format!("Unknown command: {command}");
        }
    }
    Ok(())
}

async fn approve_selected_approval(client: &TuiDaemonClient, state: &mut AppState) -> Result<()> {
    let approval_id = state
        .selected_approval()
        .map(|approval| approval.approval_id.clone())
        .ok_or_else(|| anyhow::anyhow!("No approval selected"))?;
    approve_named_approval(client, state, &approval_id).await
}

async fn reject_selected_approval(client: &TuiDaemonClient, state: &mut AppState) -> Result<()> {
    let approval_id = state
        .selected_approval()
        .map(|approval| approval.approval_id.clone())
        .ok_or_else(|| anyhow::anyhow!("No approval selected"))?;
    reject_named_approval(client, state, &approval_id, None).await
}

async fn approve_named_approval(
    client: &TuiDaemonClient,
    state: &mut AppState,
    approval_id: &str,
) -> Result<()> {
    let team_run_id = state
        .current_team_state
        .as_ref()
        .map(|team| team.team_run_id.clone())
        .ok_or_else(|| anyhow::anyhow!("No active team context for approval"))?;
    if approval_id.trim().is_empty() {
        bail!("Usage: /approve <approval_id>");
    }
    let output = client
        .execute_runtime_tool(
            "manage_teams",
            json!({
                "operation": "resolve_team_approval",
                "team_run_id": team_run_id,
                "approval_id": approval_id,
                "approved": true,
            }),
        )
        .await?;
    if !output.success {
        bail!(output.error.unwrap_or_else(|| "approval failed".to_string()));
    }
    if let Some(team_run_id) = state.current_team_state.as_ref().map(|team| team.team_run_id.clone()) {
        load_team_state(client, state, &team_run_id).await?;
    }
    state.status = format!("Approved {approval_id}");
    Ok(())
}

async fn reject_named_approval(
    client: &TuiDaemonClient,
    state: &mut AppState,
    approval_id: &str,
    reason: Option<String>,
) -> Result<()> {
    let team_run_id = state
        .current_team_state
        .as_ref()
        .map(|team| team.team_run_id.clone())
        .ok_or_else(|| anyhow::anyhow!("No active team context for rejection"))?;
    if approval_id.trim().is_empty() {
        bail!("Usage: /reject <approval_id> [reason]");
    }
    let output = client
        .execute_runtime_tool(
            "manage_teams",
            json!({
                "operation": "resolve_team_approval",
                "team_run_id": team_run_id,
                "approval_id": approval_id,
                "approved": false,
                "reason": reason,
            }),
        )
        .await?;
    if !output.success {
        bail!(output.error.unwrap_or_else(|| "reject failed".to_string()));
    }
    if let Some(team_run_id) = state.current_team_state.as_ref().map(|team| team.team_run_id.clone()) {
        load_team_state(client, state, &team_run_id).await?;
    }
    state.status = format!("Rejected {approval_id}");
    Ok(())
}

async fn load_team_state(client: &TuiDaemonClient, state: &mut AppState, team_run_id: &str) -> Result<()> {
    let state_result = client
        .execute_runtime_tool(
            "manage_teams",
            json!({
                "operation": "get_team_state",
                "team_run_id": team_run_id,
            }),
        )
        .await?;
    if !state_result.success {
        bail!(
            "{}",
            state_result
                .error
                .unwrap_or_else(|| "get_team_state failed".to_string())
        );
    }
    state.current_team_state = serde_json::from_value(state_result.result["team"].clone()).ok();

    let messages_result = client
        .execute_runtime_tool(
            "manage_teams",
            json!({
                "operation": "list_team_messages",
                "team_run_id": team_run_id,
            }),
        )
        .await?;
    if messages_result.success {
        state.current_team_messages =
            serde_json::from_value(messages_result.result["messages"].clone()).unwrap_or_default();
        let approval_texts = state
            .current_team_messages
            .iter()
            .filter(|message| message.kind == restflow_traits::TeamMessageKind::ApprovalRequest)
            .map(|message| format!("team approval request: {}", message.content))
            .collect::<Vec<_>>();
        for text in approval_texts {
            state.push_transcript(TranscriptKind::Info, text);
        }
    }

    let assignments_result = client
        .execute_runtime_tool(
            "manage_teams",
            json!({
                "operation": "list_team_assignments",
                "team_run_id": team_run_id,
            }),
        )
        .await?;
    if assignments_result.success {
        state.current_team_assignments =
            serde_json::from_value(assignments_result.result["assignments"].clone())
                .unwrap_or_default();
    }

    state.current_team_approvals = state
        .current_team_messages
        .iter()
        .filter(|message| message.kind == restflow_traits::TeamMessageKind::ApprovalRequest)
        .map(|message| PendingTeamApproval {
            team_run_id: message.team_run_id.clone(),
            approval_id: message
                .content
                .split_whitespace()
                .last()
                .unwrap_or_default()
                .trim_matches(|ch| ch == '(' || ch == ')')
                .to_string(),
            member_id: message.from_member_id.clone(),
            tool_name: "unknown".to_string(),
            content: message.content.clone(),
            status: restflow_traits::TeamApprovalStatus::Pending,
            requested_at: message.created_at,
            resolved_at: None,
            resolution_reason: None,
        })
        .collect();

    state.status = format!("Loaded team {team_run_id}");
    Ok(())
}

fn session_id_of(event: &ChatSessionEvent) -> &str {
    match event {
        ChatSessionEvent::Created { session_id }
        | ChatSessionEvent::Updated { session_id }
        | ChatSessionEvent::MessageAdded { session_id, .. }
        | ChatSessionEvent::Deleted { session_id } => session_id,
    }
}
