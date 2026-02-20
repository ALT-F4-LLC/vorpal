//! TUI event loop and terminal management.
//!
//! Sets up the terminal (raw mode, alternate screen), runs the main event loop
//! dispatching input and agent events, and ensures proper terminal teardown on
//! exit or panic. Keyboard input handling lives in the sibling [`super::input`]
//! module.

use super::input;
use super::manager::{AgentManager, ClaudeOptions};
use super::state::{
    self, AgentActivity, AgentEvent, AgentState, AgentStatus, App, DisplayLine, ToastKind,
};
use super::ui;
use anyhow::Result;
use crossterm::{
    event::{
        DisableMouseCapture, EnableMouseCapture, Event, EventStream, KeyEventKind,
        KeyboardEnhancementFlags, PopKeyboardEnhancementFlags, PushKeyboardEnhancementFlags,
    },
    execute,
    terminal::{self, supports_keyboard_enhancement, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_lite::StreamExt;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io::{self, stdout, Stdout};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

// ---------------------------------------------------------------------------
// Terminal setup / teardown
// ---------------------------------------------------------------------------

/// Initialize the terminal for TUI rendering.
///
/// Enables raw mode, switches to the alternate screen, enables mouse
/// capture, and (when the terminal supports it) enables the Kitty keyboard
/// protocol so modifier keys on Enter/Backspace/etc. are reported.
/// Returns the terminal and a flag indicating whether keyboard enhancement
/// was enabled (needed for correct teardown).
fn setup_terminal() -> Result<(Terminal<CrosstermBackend<Stdout>>, bool)> {
    terminal::enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;

    // Enable the Kitty keyboard protocol when the terminal supports it.
    // This lets crossterm report Shift/Ctrl/Alt modifiers on keys like
    // Enter that are otherwise indistinguishable from their unmodified
    // variants.
    let kbd_enhanced = supports_keyboard_enhancement().unwrap_or(false);
    if kbd_enhanced {
        execute!(
            stdout,
            PushKeyboardEnhancementFlags(
                KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                    | KeyboardEnhancementFlags::REPORT_EVENT_TYPES
            )
        )?;
    }

    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok((terminal, kbd_enhanced))
}

/// Restore the terminal to its original state.
///
/// Pops keyboard enhancement flags (if they were pushed), disables mouse
/// capture, disables raw mode, leaves the alternate screen, and shows the
/// cursor.
fn restore_terminal(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    kbd_enhanced: bool,
) -> Result<()> {
    if kbd_enhanced {
        execute!(terminal.backend_mut(), PopKeyboardEnhancementFlags)?;
    }
    terminal::disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        DisableMouseCapture,
        LeaveAlternateScreen
    )?;
    terminal.show_cursor()?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Panic hook guard (RAII)
// ---------------------------------------------------------------------------

/// The type returned by [`std::panic::take_hook`] and accepted by
/// [`std::panic::set_hook`].
type PanicHook = Box<dyn Fn(&std::panic::PanicHookInfo<'_>) + Send + Sync + 'static>;

/// RAII guard that restores the original panic hook when dropped.
///
/// Installs a TUI-aware panic hook on creation that restores terminal state
/// (raw mode, alternate screen) before printing the panic info. When dropped,
/// the original hook is reinstated — even on early return or unwinding.
struct PanicGuard {
    original: Option<PanicHook>,
}

impl PanicGuard {
    /// Replace the current panic hook with one that restores terminal state,
    /// returning a guard that will reinstate the original hook on drop.
    fn install() -> Self {
        let original = std::panic::take_hook();
        std::panic::set_hook(Box::new(|panic_info| {
            let _ = terminal::disable_raw_mode();
            let _ = execute!(io::stdout(), DisableMouseCapture, LeaveAlternateScreen);
            eprintln!("{panic_info}");
        }));
        Self {
            original: Some(original),
        }
    }
}

impl Drop for PanicGuard {
    fn drop(&mut self) {
        if let Some(hook) = self.original.take() {
            // Replace our TUI hook with the original, discarding the TUI hook.
            let _ = std::panic::take_hook();
            std::panic::set_hook(hook);
        }
    }
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Run the agent TUI.
///
/// Installs a panic hook guard (RAII), sets up the terminal, spawns initial
/// agents from the given prompts and workspaces, and enters the main event
/// loop. On exit (normal, error, or panic), the terminal and panic hook are
/// restored automatically via the guard's `Drop` implementation.
pub async fn run(
    prompts: Vec<String>,
    workspaces: Vec<PathBuf>,
    claude_options: ClaudeOptions,
) -> Result<()> {
    info!(agent_count = prompts.len(), "starting agent TUI");

    // Install panic hook guard — restores the original hook on drop (RAII).
    let _panic_guard = PanicGuard::install();

    let (mut terminal, kbd_enhanced) = setup_terminal()?;
    debug!(kbd_enhanced, "terminal setup complete");

    // Set up the event channel and agent manager.
    const EVENT_CHANNEL_CAPACITY: usize = 10_000;
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(EVENT_CHANNEL_CAPACITY);
    let mut manager = AgentManager::new(event_tx);
    let mut app = App::new(
        std::env::current_dir().unwrap_or_default(),
        claude_options.clone(),
    );
    app.kbd_enhanced = kbd_enhanced;

    // Spawn initial agents from prompts/workspaces pairs.
    for (prompt, workspace) in prompts.iter().zip(workspaces.iter()) {
        let agent_id = manager
            .spawn(prompt, workspace, &claude_options, None)
            .await?;
        app.add_agent(AgentState::new(
            agent_id,
            String::new(),
            workspace.clone(),
            prompt.clone(),
            claude_options.clone(),
        ));
    }

    // Create async event stream for crossterm terminal input.
    let mut reader = EventStream::new();

    // Run the event loop, capturing the result so we always clean up.
    let result = event_loop(
        &mut terminal,
        &mut app,
        &mut manager,
        &mut event_rx,
        &mut reader,
    )
    .await;

    // Cleanup: kill all agents and restore terminal state.
    info!("shutting down agent TUI");
    manager.kill_all().await;
    restore_terminal(&mut terminal, kbd_enhanced)?;
    debug!("terminal restored");

    // _panic_guard drops here, restoring the original panic hook.
    result
}

// ---------------------------------------------------------------------------
// Event loop
// ---------------------------------------------------------------------------

/// Minimum interval between consecutive draws (~60 fps).
const DRAW_INTERVAL: Duration = Duration::from_millis(16);

/// Main event loop.
///
/// Multiplexes between agent output events and terminal keyboard input using
/// [`tokio::select!`]. Rendering is throttled to approximately 60 fps: events
/// are batched between draws, and `terminal.draw()` is only called when at
/// least [`DRAW_INTERVAL`] has elapsed since the last draw. A short sleep is
/// used when the frame budget has not yet expired, which also drives spinner
/// animation.
async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    manager: &mut AgentManager,
    event_rx: &mut mpsc::Receiver<AgentEvent>,
    reader: &mut EventStream,
) -> Result<()> {
    let mut tick_interval = tokio::time::interval(Duration::from_millis(100));
    let mut last_draw = Instant::now() - DRAW_INTERVAL;
    let mut needs_draw = true;

    loop {
        // Draw at most once per DRAW_INTERVAL (~60 fps).
        if needs_draw {
            let elapsed = last_draw.elapsed();
            if elapsed >= DRAW_INTERVAL {
                terminal.draw(|frame| ui::render(app, frame))?;
                last_draw = Instant::now();
                needs_draw = false;
            }
        }

        // Compute remaining time until next allowed draw for the sleep branch.
        let until_next_draw = DRAW_INTERVAL.saturating_sub(last_draw.elapsed());

        tokio::select! {
            _ = tick_interval.tick() => {
                app.tick = app.tick.wrapping_add(1);
                app.expire_toasts();
                needs_draw = true;
            }
            Some(event) = event_rx.recv() => {
                handle_app_event(app, manager, event).await;
                needs_draw = true;
            }
            Some(result) = reader.next() => {
                match result {
                    Ok(Event::Key(key_event)) => {
                        // Only handle key press events (ignore release/repeat).
                        if key_event.kind == KeyEventKind::Press {
                            input::handle_key(app, manager, key_event).await;
                            needs_draw = true;
                        }
                    }
                    Ok(Event::Mouse(mouse_event)) if app.mouse_enabled => {
                        if input::handle_mouse(app, mouse_event).await {
                            needs_draw = true;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "terminal input read error");
                    }
                    _ => {}
                }
            }
            // Sleep until the frame budget expires so we can draw the
            // batched updates. This also keeps spinner animation smooth.
            _ = tokio::time::sleep(until_next_draw), if needs_draw => {
                // Timer expired — loop back to the draw check at the top.
            }
            // Both channels closed — exit gracefully.
            else => break,
        }

        if app.should_quit {
            break;
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Event handlers
// ---------------------------------------------------------------------------

/// Handle an application event from the agent event channel.
async fn handle_app_event(app: &mut App, manager: &mut AgentManager, event: AgentEvent) {
    match event {
        AgentEvent::Output { agent_id, mut line } => {
            // Fix the placeholder "agent" sender on AgentMessage lines by
            // resolving the sending agent's unique name.
            if let DisplayLine::AgentMessage { sender, .. } = &mut line {
                if sender == "agent" {
                    let actual = app
                        .agent_by_id_mut(agent_id)
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| format!("agent-{}", agent_id + 1));
                    *sender = actual;
                }
            }

            // Extract cross-agent delivery data before pushing to the sender.
            let cross_delivery = if let DisplayLine::AgentMessage {
                sender,
                recipient,
                content,
            } = &line
            {
                Some((sender.clone(), recipient.clone(), content.clone()))
            } else {
                None
            };

            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                match &line {
                    DisplayLine::TurnStart => {
                        agent.activity = AgentActivity::Thinking;
                        agent.turn_count += 1;
                    }
                    DisplayLine::ToolUse { tool, .. } => {
                        agent.activity = AgentActivity::Tool(tool.clone());
                        agent.tool_count += 1;
                    }
                    DisplayLine::Text(_) => {
                        agent.activity = AgentActivity::Thinking;
                    }
                    DisplayLine::Result(_) => {
                        agent.activity = AgentActivity::Done;
                    }
                    _ => {}
                }
                agent.push_line(line);
            }

            // Deliver a copy of the message to the recipient agent's output
            // so inter-agent messages are visible on both sides.
            if let Some((sender, recipient, content)) = cross_delivery {
                let recipient_idx = app
                    .agents
                    .iter()
                    .position(|a| state::agent_name_matches(&a.name, &recipient));
                if let Some(idx) = recipient_idx {
                    let recv_id = app.agents[idx].id;
                    if recv_id != agent_id {
                        app.agents[idx].push_line(DisplayLine::AgentMessage {
                            sender,
                            recipient,
                            content,
                        });
                    }
                }
            }
        }
        AgentEvent::Exited {
            agent_id,
            exit_code,
        } => {
            info!(agent_id, ?exit_code, "agent exited");

            // Determine the Vec index and whether this agent is unfocused
            // before mutating state so we can generate a toast.
            let vec_index = app.agent_vec_index(agent_id);
            let is_unfocused = vec_index.is_some_and(|idx| idx != app.focused);

            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                agent.status = AgentStatus::Exited(exit_code);
                agent.activity = AgentActivity::Done;
                agent.pause_timer();
            }
            manager.notify_exited(agent_id);

            // Toast + unread indicator for unfocused agents.
            if is_unfocused {
                if let Some(idx) = vec_index {
                    let agent_num = idx + 1;
                    let (msg, kind) = match exit_code {
                        Some(0) => (
                            format!("Agent {agent_num} completed successfully"),
                            ToastKind::Success,
                        ),
                        Some(code) => (
                            format!("Agent {agent_num} failed (exit code {code})"),
                            ToastKind::Error,
                        ),
                        None => (
                            format!("Agent {agent_num} exited (unknown status)"),
                            ToastKind::Error,
                        ),
                    };
                    app.push_toast(msg, kind);
                    app.unread_agents.insert(agent_id);
                }
            }

            // Queue drain: auto-submit next queued message for this agent,
            // but only if the agent exited successfully (exit code 0).
            // Failed agents keep their queue intact so users can inspect
            // the failure and decide whether to retry or clear the queue.
            //
            // We spawn directly via the manager rather than routing through
            // the input form (enter_input_mode_with / submit_and_spawn) to
            // avoid a transient InputMode::Input state that could leave the
            // UI in a broken mode if the spawn fails.
            if exit_code == Some(0) {
                if let Some(vec_idx) = app.agent_vec_index(agent_id) {
                    let should_drain = app
                        .agents
                        .get(vec_idx)
                        .is_some_and(|a| !a.message_queue.is_empty() && a.session_id.is_some());
                    if should_drain {
                        let agent = &mut app.agents[vec_idx];
                        let queued_msg = agent.message_queue.pop_front().unwrap();
                        let opts = agent.claude_options.clone();
                        let workspace = agent.workspace.clone();
                        let session_id = agent.session_id.clone();
                        let remaining = agent.message_queue.len();

                        match manager
                            .spawn(&queued_msg, &workspace, &opts, session_id.as_deref())
                            .await
                        {
                            Ok(new_agent_id) => {
                                app.save_to_history(&queued_msg, &workspace, &opts);
                                if let Some(agent) = app.agent_by_id_mut(agent_id) {
                                    agent.push_line(DisplayLine::TurnStart);
                                    agent.id = new_agent_id;
                                    agent.status = AgentStatus::Running;
                                    agent.activity = AgentActivity::Idle;
                                    agent.prompt = queued_msg;
                                    agent.scroll_offset = 0;
                                    agent.has_new_output = false;
                                    agent.cached_lines = None;
                                    agent.resume_timer();
                                    app.rebuild_agent_index();
                                } else {
                                    warn!(
                                        agent_id,
                                        new_agent_id, "queue drain: agent disappeared after spawn"
                                    );
                                }
                                if remaining > 0 {
                                    app.set_status_message(format!(
                                        "Auto-submitted queued message ({remaining} remaining)"
                                    ));
                                } else {
                                    app.set_status_message("Auto-submitted queued message");
                                }
                            }
                            Err(e) => {
                                // Re-queue the message so the user doesn't lose it.
                                if let Some(agent) = app.agent_by_id_mut(agent_id) {
                                    agent.message_queue.push_front(queued_msg);
                                }
                                app.set_status_message(format!("Queue drain failed: {e}"));
                            }
                        }
                    }
                }
            } else if let Some(vec_idx) = app.agent_vec_index(agent_id) {
                let queue_len = app
                    .agents
                    .get(vec_idx)
                    .map(|a| a.message_queue.len())
                    .unwrap_or(0);
                if queue_len > 0 {
                    app.set_status_message(format!(
                        "Agent failed — {queue_len} queued message(s) preserved (use :clear-queue to discard)"
                    ));
                }
            }
        }
        AgentEvent::SessionId {
            agent_id,
            session_id,
        } => {
            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                agent.session_id = Some(session_id);
            }
        }
        AgentEvent::UsageUpdate {
            agent_id,
            input_tokens,
            output_tokens,
            total_cost_usd,
        } => {
            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                agent.input_tokens += input_tokens;
                agent.output_tokens += output_tokens;
                if let Some(cost) = total_cost_usd {
                    agent.total_cost_usd += cost;
                }
            }
        }
    }
}
