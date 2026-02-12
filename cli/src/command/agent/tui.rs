//! TUI event loop and terminal management.
//!
//! Sets up the terminal (raw mode, alternate screen), runs the main event loop
//! dispatching input and agent events, and ensures proper terminal teardown on
//! exit or panic. Keyboard input handling lives in the sibling [`super::input`]
//! module.

use super::input;
use super::manager::{AgentManager, ClaudeOptions};
use super::state::{AgentActivity, AgentEvent, AgentState, AgentStatus, App, DisplayLine};
use super::ui;
use anyhow::Result;
use crossterm::{
    event::{Event, EventStream, KeyEventKind},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
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
/// Enables raw mode and switches to the alternate screen.
fn setup_terminal() -> Result<Terminal<CrosstermBackend<Stdout>>> {
    terminal::enable_raw_mode()?;
    let mut stdout = stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let terminal = Terminal::new(backend)?;
    Ok(terminal)
}

/// Restore the terminal to its original state.
///
/// Disables raw mode, leaves the alternate screen, and shows the cursor.
fn restore_terminal(terminal: &mut Terminal<CrosstermBackend<Stdout>>) -> Result<()> {
    terminal::disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
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
            let _ = execute!(io::stdout(), LeaveAlternateScreen);
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

    let mut terminal = setup_terminal()?;
    debug!("terminal setup complete");

    // Set up the event channel and agent manager.
    const EVENT_CHANNEL_CAPACITY: usize = 10_000;
    let (event_tx, mut event_rx) = mpsc::channel::<AgentEvent>(EVENT_CHANNEL_CAPACITY);
    let mut manager = AgentManager::new(event_tx);
    let mut app = App::new(
        std::env::current_dir().unwrap_or_default(),
        claude_options.clone(),
    );

    // Spawn initial agents from prompts/workspaces pairs.
    for (prompt, workspace) in prompts.iter().zip(workspaces.iter()) {
        let agent_id = manager
            .spawn(prompt, workspace, &claude_options, None)
            .await?;
        app.add_agent(AgentState::new(
            agent_id,
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
    restore_terminal(&mut terminal)?;
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
                needs_draw = true;
            }
            Some(event) = event_rx.recv() => {
                handle_app_event(app, manager, event);
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
fn handle_app_event(app: &mut App, manager: &mut AgentManager, event: AgentEvent) {
    match event {
        AgentEvent::Output { agent_id, line } => {
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
        }
        AgentEvent::Exited {
            agent_id,
            exit_code,
        } => {
            info!(agent_id, ?exit_code, "agent exited");
            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                agent.status = AgentStatus::Exited(exit_code);
                agent.activity = AgentActivity::Done;
            }
            manager.notify_exited(agent_id);
        }
        AgentEvent::SessionId {
            agent_id,
            session_id,
        } => {
            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                agent.session_id = Some(session_id);
            }
        }
    }
}
