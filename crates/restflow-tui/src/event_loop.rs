use std::collections::VecDeque;
use std::thread;
use std::time::Duration;

use anyhow::Result;
use crossterm::event::{self, Event};
use tokio::sync::mpsc;

use super::controller::ShellController;
use super::keymap::{Action, map_event};
use super::reducer::{ShellAction, ShellEffect, reduce};
use super::shell::ShellRenderer;
use super::state::AppState;

use restflow_core::daemon::{ChatSessionEvent, StreamFrame};
use restflow_core::runtime::TaskStreamEvent;

const MAX_BATCHED_INPUT_EVENTS: usize = 64;
const RENDER_FRAME_INTERVAL: Duration = Duration::from_millis(16);

#[derive(Debug)]
pub enum AppEvent {
    Terminal(Event),
    StreamFrame(StreamFrame),
    SessionEvent(ChatSessionEvent),
    TaskEvent(TaskStreamEvent),
    Error(String),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ProcessActionsResult {
    should_quit: bool,
    render_request: RenderRequest,
    immediate_render: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct RenderRequest {
    full: bool,
    viewport: bool,
}

impl RenderRequest {
    fn viewport() -> Self {
        Self {
            full: false,
            viewport: true,
        }
    }

    fn full() -> Self {
        Self {
            full: true,
            viewport: false,
        }
    }

    fn merge(&mut self, other: Self) {
        self.full |= other.full;
        self.viewport |= other.viewport;
        if self.full {
            self.viewport = false;
        }
    }
}

pub async fn run_event_loop(controller: ShellController, mut state: AppState) -> Result<()> {
    let mut renderer = ShellRenderer::new();
    renderer.clear_screen()?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let _input_handle = spawn_input_thread(tx.clone());
    let mut session_stream_handle = if state.is_startup_mode() {
        None
    } else {
        Some(controller.spawn_session_events(tx.clone()))
    };
    let mut selected_task_stream: Option<(String, tokio::task::JoinHandle<()>)> = None;
    let mut pending_events = VecDeque::new();
    let mut render_request = RenderRequest::full();

    let mut result = process_actions(
        &controller,
        &mut renderer,
        &mut state,
        VecDeque::from([ShellAction::RefreshTick]),
        tx.clone(),
    )
    .await?;
    if result.should_quit {
        return Ok(());
    }
    render_request.merge(result.render_request);
    if result.immediate_render {
        renderer.sync(&state)?;
        render_request = RenderRequest::default();
    }

    if let Some(message) = state.take_pending_initial_message() {
        result = process_actions(
            &controller,
            &mut renderer,
            &mut state,
            VecDeque::from([ShellAction::SubmitText { text: message }]),
            tx.clone(),
        )
        .await?;
        if result.should_quit {
            return Ok(());
        }
        render_request.merge(result.render_request);
        if result.immediate_render {
            renderer.sync(&state)?;
            render_request = RenderRequest::default();
        }
    }

    let mut tick = tokio::time::interval(Duration::from_secs(3));
    let mut render_tick = tokio::time::interval(RENDER_FRAME_INTERVAL);
    render_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            _ = render_tick.tick() => {
                if render_request.full {
                    renderer.sync(&state)?;
                    render_request = RenderRequest::default();
                } else if render_request.viewport {
                    renderer.sync_viewport_only(&state)?;
                    render_request = RenderRequest::default();
                }
            }
            _ = tick.tick() => {
                let result = process_actions(
                    &controller,
                    &mut renderer,
                    &mut state,
                    VecDeque::from([ShellAction::RefreshTick]),
                    tx.clone(),
                )
                .await?;
                if result.should_quit {
                    break;
                }
                render_request.merge(result.render_request);
                if result.immediate_render {
                    renderer.sync(&state)?;
                    render_request = RenderRequest::default();
                }
            }
            maybe_event = next_event(&mut rx, &mut pending_events) => {
                let Some(event) = maybe_event else { break; };
                let actions = collect_action_batch(event, &mut rx, &mut pending_events);
                let result = process_actions(&controller, &mut renderer, &mut state, actions, tx.clone()).await?;
                if result.should_quit {
                    break;
                }
                render_request.merge(result.render_request);
                if result.immediate_render {
                    renderer.sync(&state)?;
                    render_request = RenderRequest::default();
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
    thread::spawn(move || {
        loop {
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
        }
    })
}

async fn next_event(
    rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    pending_events: &mut VecDeque<AppEvent>,
) -> Option<AppEvent> {
    if let Some(event) = pending_events.pop_front() {
        Some(event)
    } else {
        rx.recv().await
    }
}

fn collect_action_batch(
    first_event: AppEvent,
    rx: &mut mpsc::UnboundedReceiver<AppEvent>,
    pending_events: &mut VecDeque<AppEvent>,
) -> VecDeque<ShellAction> {
    let first_action = app_event_to_action(first_event);
    if !is_batchable_input_action(&first_action) {
        return VecDeque::from([first_action]);
    }

    let mut actions = VecDeque::from([first_action]);
    while actions.len() < MAX_BATCHED_INPUT_EVENTS {
        match rx.try_recv() {
            Ok(event) => match event {
                AppEvent::Terminal(event) => {
                    let action = ShellAction::Ui(map_event(event.clone()));
                    if is_batchable_input_action(&action) {
                        actions.push_back(action);
                    } else {
                        pending_events.push_back(AppEvent::Terminal(event));
                        break;
                    }
                }
                other => {
                    pending_events.push_back(other);
                    break;
                }
            },
            Err(_) => break,
        }
    }
    actions
}

fn app_event_to_action(event: AppEvent) -> ShellAction {
    match event {
        AppEvent::Terminal(event) => ShellAction::Ui(map_event(event)),
        AppEvent::StreamFrame(frame) => ShellAction::StreamFrame(frame),
        AppEvent::SessionEvent(event) => ShellAction::SessionEvent(event),
        AppEvent::TaskEvent(event) => ShellAction::TaskEvent(event),
        AppEvent::Error(message) => ShellAction::Error(message),
    }
}

fn is_batchable_input_action(action: &ShellAction) -> bool {
    matches!(
        action,
        ShellAction::Ui(
            Action::InputChar(_)
                | Action::InputBackspace
                | Action::MoveLeft
                | Action::MoveRight
                | Action::Newline
                | Action::Paste(_)
        )
    )
}

async fn process_actions(
    controller: &ShellController,
    renderer: &mut ShellRenderer,
    state: &mut AppState,
    actions: VecDeque<ShellAction>,
    tx: mpsc::UnboundedSender<AppEvent>,
) -> Result<ProcessActionsResult> {
    let mut pending = actions;
    let mut output = ProcessActionsResult::default();

    while let Some(next_action) = pending.pop_front() {
        output
            .render_request
            .merge(render_request_for_action(&next_action));
        output.immediate_render |= action_requires_immediate_render(&next_action);

        let result = reduce(state, next_action);
        if result.should_quit {
            output.should_quit = true;
            return Ok(output);
        }

        pending.extend(result.actions);

        for effect in result.effects {
            output.render_request.merge(RenderRequest::full());
            output.immediate_render = true;
            if matches!(effect, ShellEffect::ClearScreen) {
                renderer.clear_screen()?;
                continue;
            }

            let followup_actions = controller.execute_effect(effect, state, tx.clone()).await?;
            pending.extend(followup_actions);
        }
    }

    Ok(output)
}

fn render_request_for_action(action: &ShellAction) -> RenderRequest {
    if matches!(action, ShellAction::Ui(Action::Noop)) {
        RenderRequest::default()
    } else if is_batchable_input_action(action) {
        RenderRequest::viewport()
    } else {
        RenderRequest::full()
    }
}

fn action_requires_immediate_render(action: &ShellAction) -> bool {
    !matches!(
        action,
        ShellAction::Ui(
            Action::InputChar(_)
                | Action::InputBackspace
                | Action::MoveLeft
                | Action::MoveRight
                | Action::Newline
                | Action::Paste(_)
                | Action::Noop
        )
    )
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
            if current.is_some()
                && let Some((_, handle)) = slot.take()
            {
                handle.abort();
            }
            *slot = Some((
                desired.clone(),
                controller.spawn_task_events(desired, tx.clone()),
            ));
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

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};

    use super::*;

    fn key(code: KeyCode) -> AppEvent {
        AppEvent::Terminal(Event::Key(KeyEvent::new(code, KeyModifiers::NONE)))
    }

    #[test]
    fn collect_action_batch_drains_contiguous_input_events() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_events = VecDeque::new();
        tx.send(key(KeyCode::Char('b'))).unwrap();
        tx.send(key(KeyCode::Char('c'))).unwrap();
        tx.send(key(KeyCode::Enter)).unwrap();

        let actions = collect_action_batch(key(KeyCode::Char('a')), &mut rx, &mut pending_events);

        let input = actions
            .into_iter()
            .map(|action| match action {
                ShellAction::Ui(Action::InputChar(ch)) => ch,
                other => panic!("unexpected action in input batch: {other:?}"),
            })
            .collect::<String>();
        assert_eq!(input, "abc");
        assert!(matches!(
            pending_events.pop_front(),
            Some(AppEvent::Terminal(Event::Key(event))) if event.code == KeyCode::Enter
        ));
    }

    #[test]
    fn collect_action_batch_does_not_batch_submit_first() {
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_events = VecDeque::new();

        let actions = collect_action_batch(key(KeyCode::Enter), &mut rx, &mut pending_events);

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions.front(),
            Some(ShellAction::Ui(Action::Submit))
        ));
        assert!(pending_events.is_empty());
    }

    #[test]
    fn collect_action_batch_stops_before_non_terminal_event() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_events = VecDeque::new();
        tx.send(AppEvent::Error("boom".to_string())).unwrap();

        let actions = collect_action_batch(key(KeyCode::Char('a')), &mut rx, &mut pending_events);

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions.front(),
            Some(ShellAction::Ui(Action::InputChar('a')))
        ));
        assert!(matches!(
            pending_events.pop_front(),
            Some(AppEvent::Error(message)) if message == "boom"
        ));
    }

    #[test]
    fn input_actions_are_dirty_but_not_immediate() {
        let action = ShellAction::Ui(Action::InputChar('x'));
        let request = render_request_for_action(&action);

        assert_eq!(request, RenderRequest::viewport());
        assert!(request.viewport);
        assert!(!request.full);
        assert!(!action_requires_immediate_render(&action));
    }

    #[test]
    fn submit_and_resize_actions_render_immediately() {
        assert!(action_requires_immediate_render(&ShellAction::Ui(
            Action::Submit
        )));
        assert!(action_requires_immediate_render(&ShellAction::Ui(
            Action::Resize
        )));
    }
}
