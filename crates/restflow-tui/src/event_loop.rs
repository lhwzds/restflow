use std::io;
use std::thread;
use std::time::Duration;
use std::collections::VecDeque;

use anyhow::Result;
use crossterm::event::{self, Event};
use ratatui::Terminal;
use ratatui::backend::CrosstermBackend;
use tokio::sync::mpsc;

use super::controller::ShellController;
use super::keymap::map_event;
use super::reducer::{ShellAction, ShellEffect, reduce};
use super::render;
use super::state::AppState;

use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::runtime::TaskStreamEvent;

#[derive(Debug)]
pub enum AppEvent {
    Terminal(Event),
    StreamFrame(StreamFrame),
    SessionEvent(ChatSessionEvent),
    TaskEvent(TaskStreamEvent),
    Error(String),
}

pub async fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    controller: ShellController,
    mut state: AppState,
) -> Result<()> {
    let (tx, mut rx) = mpsc::unbounded_channel();
    let _input_handle = spawn_input_thread(tx.clone());
    let mut session_stream_handle = if state.is_startup_mode() {
        None
    } else {
        Some(controller.spawn_session_events(tx.clone()))
    };
    let mut selected_task_stream: Option<(String, tokio::task::JoinHandle<()>)> = None;

    if process_action(
        &controller,
        terminal,
        &mut state,
        ShellAction::RefreshTick,
        tx.clone(),
    )
    .await? {
        return Ok(());
    }
    if let Some(message) = state.take_pending_initial_message()
        && process_action(
            &controller,
            terminal,
            &mut state,
            ShellAction::SubmitText { text: message },
            tx.clone(),
        )
        .await?
    {
        return Ok(());
    }

    let mut tick = tokio::time::interval(Duration::from_secs(3));

    loop {
        terminal.draw(|frame| render::render(frame, &state))?;

        tokio::select! {
            _ = tick.tick() => {
                if process_action(
                    &controller,
                    terminal,
                    &mut state,
                    ShellAction::RefreshTick,
                    tx.clone(),
                )
                .await? {
                    break;
                }
            }
            maybe_event = rx.recv() => {
                let Some(event) = maybe_event else { break; };
                let action = match event {
                    AppEvent::Terminal(event) => ShellAction::Ui(map_event(event)),
                    AppEvent::StreamFrame(frame) => ShellAction::StreamFrame(frame),
                    AppEvent::SessionEvent(event) => ShellAction::SessionEvent(event),
                    AppEvent::TaskEvent(event) => ShellAction::TaskEvent(event),
                    AppEvent::Error(message) => ShellAction::Error(message),
                };
                if process_action(&controller, terminal, &mut state, action, tx.clone()).await? {
                    break;
                }
            }
        }

        sync_task_subscription(&controller, &state, &tx, &mut selected_task_stream);
        sync_session_subscription(&controller, &state, &tx, &mut session_stream_handle);
    }

    if let Some(handle) = session_stream_handle.take() {
        handle.abort();
    }
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

async fn process_action(
    controller: &ShellController,
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    state: &mut AppState,
    action: ShellAction,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<bool> {
    let mut pending = VecDeque::from([action]);

    while let Some(next_action) = pending.pop_front() {
        let result = reduce(state, next_action);
        if result.should_quit {
            return Ok(true);
        }

        pending.extend(result.actions);

        for effect in result.effects {
            if matches!(effect, ShellEffect::ClearScreen) {
                terminal.clear()?;
                continue;
            }

            let followup_actions = controller.execute_effect(effect, state, tx.clone()).await?;
            pending.extend(followup_actions);
        }
    }

    Ok(false)
}

fn sync_task_subscription(
    controller: &ShellController,
    state: &AppState,
    tx: &mpsc::UnboundedSender<AppEvent>,
    slot: &mut Option<(String, tokio::task::JoinHandle<()>)>,
) {
    let desired = state.focused_task_stream_id().map(ToOwned::to_owned);
    match (slot.as_ref().map(|(id, _)| id.clone()), desired) {
        (Some(current), Some(desired)) if current == desired => {}
        (current, Some(desired)) => {
            if current.is_some() && let Some((_, handle)) = slot.take() {
                handle.abort();
            }
            *slot = Some((desired.clone(), controller.spawn_task_events(desired, tx.clone())));
        }
        (Some(_), None) => {
            if let Some((_, handle)) = slot.take() {
                handle.abort();
            }
        }
        (None, None) => {}
    }
}

fn sync_session_subscription(
    controller: &ShellController,
    state: &AppState,
    tx: &mpsc::UnboundedSender<AppEvent>,
    slot: &mut Option<tokio::task::JoinHandle<()>>,
) {
    match (slot.is_some(), state.is_startup_mode()) {
        (false, false) => {
            *slot = Some(controller.spawn_session_events(tx.clone()));
        }
        (true, true) => {
            if let Some(handle) = slot.take() {
                handle.abort();
            }
        }
        _ => {}
    }
}
