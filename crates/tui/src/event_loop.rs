use std::collections::VecDeque;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::Result;
use crossterm::event::{self, Event};
use tokio::sync::mpsc;

use super::controller::ShellController;
use super::keymap::{Action, map_event};
use super::reducer::{ShellAction, ShellEffect, reduce};
use super::shell::ShellRenderer;
use super::state::AppState;

use types::{ChatSessionEvent, StreamFrame, TaskStreamEvent};

const MAX_BATCHED_INPUT_EVENTS: usize = 64;
const RENDER_FRAME_INTERVAL: Duration = Duration::from_millis(16);
const TYPING_ANIMATION_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub enum AppEvent {
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
    renderer.purge_screen()?;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let (terminal_tx, mut terminal_rx) = mpsc::unbounded_channel();
    let _input_handle = spawn_input_thread(terminal_tx);
    let mut session_stream_handle = if state.is_startup_mode() {
        None
    } else {
        Some(controller.spawn_session_events(tx.clone()))
    };
    let mut selected_task_stream: Option<(String, tokio::task::JoinHandle<()>)> = None;
    let mut pending_terminal_events = VecDeque::new();
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
        renderer.sync(&mut state)?;
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
            renderer.sync(&mut state)?;
            render_request = RenderRequest::default();
        }
    }

    let mut tick = tokio::time::interval(Duration::from_secs(3));
    let mut render_tick = tokio::time::interval(RENDER_FRAME_INTERVAL);
    let mut typing_tick = tokio::time::interval(TYPING_ANIMATION_INTERVAL);
    let mut ctrl_c = Box::pin(tokio::signal::ctrl_c());
    let mut last_active_refresh = Instant::now();
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    render_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    typing_tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        tokio::select! {
            biased;

            result = &mut ctrl_c => {
                if let Err(error) = result {
                    state.status = format!("Failed to listen for Ctrl-C: {error}");
                    render_request.merge(RenderRequest::full());
                    ctrl_c = Box::pin(tokio::signal::ctrl_c());
                    continue;
                }
                let result = process_actions(
                    &controller,
                    &mut renderer,
                    &mut state,
                    VecDeque::from([ShellAction::Ui(Action::Quit)]),
                    tx.clone(),
                )
                .await?;
                if result.should_quit {
                    break;
                }
                render_request.merge(result.render_request);
                if result.immediate_render {
                    renderer.sync(&mut state)?;
                    render_request = RenderRequest::default();
                }
                ctrl_c = Box::pin(tokio::signal::ctrl_c());
            }

            maybe_event = next_terminal_event(&mut terminal_rx, &mut pending_terminal_events) => {
                let Some(event) = maybe_event else { break; };
                let actions = collect_terminal_action_batch(event, &mut terminal_rx, &mut pending_terminal_events);
                let result = process_actions(&controller, &mut renderer, &mut state, actions, tx.clone()).await?;
                if result.should_quit {
                    break;
                }
                render_request.merge(result.render_request);
                if result.immediate_render {
                    renderer.sync(&mut state)?;
                    render_request = RenderRequest::default();
                }
            }
            maybe_event = next_event(&mut rx, &mut pending_events) => {
                let Some(event) = maybe_event else { break; };
                let actions = VecDeque::from([app_event_to_action(event)]);
                let result = process_actions(&controller, &mut renderer, &mut state, actions, tx.clone()).await?;
                if result.should_quit {
                    break;
                }
                render_request.merge(result.render_request);
                if result.immediate_render {
                    renderer.sync(&mut state)?;
                    render_request = RenderRequest::default();
                }
            }
            _ = tick.tick() => {
                last_active_refresh = Instant::now();
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
                    renderer.sync(&mut state)?;
                    render_request = RenderRequest::default();
                }
            }
            _ = typing_tick.tick() => {
                if state.update_active_typing_indicator() {
                    render_request.merge(RenderRequest::viewport());
                }
                if should_refresh_active_from_animation(&state, last_active_refresh) {
                    last_active_refresh = Instant::now();
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
                        renderer.sync(&mut state)?;
                        render_request = RenderRequest::default();
                    }
                }
            }
            _ = render_tick.tick() => {
                if render_request.full {
                    renderer.sync(&mut state)?;
                    render_request = RenderRequest::default();
                } else if render_request.viewport {
                    renderer.sync_viewport_only(&mut state)?;
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

fn spawn_input_thread(tx: mpsc::UnboundedSender<Event>) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        loop {
            if let Ok(true) = event::poll(Duration::from_millis(100)) {
                match event::read() {
                    Ok(event) => {
                        if tx.send(event).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        }
    })
}

async fn next_terminal_event(
    rx: &mut mpsc::UnboundedReceiver<Event>,
    pending_events: &mut VecDeque<Event>,
) -> Option<Event> {
    if let Some(event) = pending_events.pop_front() {
        Some(event)
    } else {
        rx.recv().await
    }
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

fn collect_terminal_action_batch(
    first_event: Event,
    rx: &mut mpsc::UnboundedReceiver<Event>,
    pending_events: &mut VecDeque<Event>,
) -> VecDeque<ShellAction> {
    let first_action = ShellAction::Ui(map_event(first_event));
    if !is_batchable_input_action(&first_action) {
        return VecDeque::from([first_action]);
    }

    let mut actions = VecDeque::from([first_action]);
    while actions.len() < MAX_BATCHED_INPUT_EVENTS {
        match rx.try_recv() {
            Ok(event) => {
                let action = ShellAction::Ui(map_event(event.clone()));
                if is_batchable_input_action(&action) {
                    actions.push_back(action);
                } else {
                    pending_events.push_back(event);
                    break;
                }
            }
            Err(_) => break,
        }
    }
    actions
}

fn app_event_to_action(event: AppEvent) -> ShellAction {
    match event {
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
            if effect_requires_pre_render(&effect) {
                output.render_request.merge(RenderRequest::full());
                output.immediate_render = true;
            }
            if matches!(effect, ShellEffect::ClearScreen) {
                renderer.clear_screen()?;
                continue;
            }

            if output.immediate_render {
                renderer.sync(state)?;
                output.render_request = RenderRequest::default();
                output.immediate_render = false;
            }

            let followup_actions = match controller.execute_effect(effect, state, tx.clone()).await
            {
                Ok(actions) => actions,
                Err(error) => vec![ShellAction::Error(error.to_string())],
            };
            pending.extend(followup_actions);
        }
    }

    Ok(output)
}

fn render_request_for_action(action: &ShellAction) -> RenderRequest {
    if matches!(
        action,
        ShellAction::Ui(Action::Noop) | ShellAction::RefreshTick
    ) {
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
        ShellAction::RefreshTick
            | ShellAction::Ui(
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

fn effect_requires_pre_render(effect: &ShellEffect) -> bool {
    !matches!(
        effect,
        ShellEffect::RefreshState | ShellEffect::ReloadCurrentSession
    )
}

fn should_refresh_active_from_animation(state: &AppState, last_refresh: Instant) -> bool {
    (state.is_streaming || state.active_turn.is_some())
        && last_refresh.elapsed() >= Duration::from_secs(1)
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

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::new(code, KeyModifiers::NONE))
    }

    #[test]
    fn collect_terminal_action_batch_drains_contiguous_input_events() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_events = VecDeque::new();
        tx.send(key(KeyCode::Char('b'))).unwrap();
        tx.send(key(KeyCode::Char('c'))).unwrap();
        tx.send(key(KeyCode::Enter)).unwrap();

        let actions =
            collect_terminal_action_batch(key(KeyCode::Char('a')), &mut rx, &mut pending_events);

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
            Some(Event::Key(event)) if event.code == KeyCode::Enter
        ));
    }

    #[test]
    fn collect_terminal_action_batch_does_not_batch_submit_first() {
        let (_tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_events = VecDeque::new();

        let actions =
            collect_terminal_action_batch(key(KeyCode::Enter), &mut rx, &mut pending_events);

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions.front(),
            Some(ShellAction::Ui(Action::Submit))
        ));
        assert!(pending_events.is_empty());
    }

    #[test]
    fn collect_terminal_action_batch_stops_before_non_batchable_terminal_event() {
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut pending_events = VecDeque::new();
        tx.send(key(KeyCode::Enter)).unwrap();

        let actions =
            collect_terminal_action_batch(key(KeyCode::Char('a')), &mut rx, &mut pending_events);

        assert_eq!(actions.len(), 1);
        assert!(matches!(
            actions.front(),
            Some(ShellAction::Ui(Action::InputChar('a')))
        ));
        assert!(matches!(
            pending_events.pop_front(),
            Some(Event::Key(event)) if event.code == KeyCode::Enter
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
    fn refresh_tick_is_not_dirty_by_itself() {
        let action = ShellAction::RefreshTick;
        let request = render_request_for_action(&action);

        assert_eq!(request, RenderRequest::default());
        assert!(!action_requires_immediate_render(&action));
    }

    #[test]
    fn animation_tick_can_drive_active_refresh_when_interval_is_due() {
        let mut state = AppState::empty();
        state.push_local_user_message("run live work".to_string());
        let last_refresh = Instant::now() - Duration::from_secs(2);

        assert!(should_refresh_active_from_animation(&state, last_refresh));
    }

    #[test]
    fn animation_tick_does_not_refresh_idle_state() {
        let state = AppState::empty();
        let last_refresh = Instant::now() - Duration::from_secs(2);

        assert!(!should_refresh_active_from_animation(&state, last_refresh));
    }

    #[test]
    fn background_refresh_effects_do_not_pre_render() {
        assert!(!effect_requires_pre_render(&ShellEffect::RefreshState));
        assert!(!effect_requires_pre_render(
            &ShellEffect::ReloadCurrentSession
        ));
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
