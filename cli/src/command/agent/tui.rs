//! TUI event loop and terminal management.
//!
//! Sets up the terminal (raw mode, alternate screen), runs the main event loop
//! dispatching input and agent events, and ensures proper terminal teardown on
//! exit or panic.

use super::manager::AgentManager;
use super::state::{AgentState, AgentStatus, App, AppEvent};
use super::ui;
use anyhow::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
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
pub async fn run(prompts: Vec<String>, workspaces: Vec<PathBuf>) -> Result<()> {
    info!(agent_count = prompts.len(), "starting agent TUI");

    // Install panic hook guard — restores the original hook on drop (RAII).
    let _panic_guard = PanicGuard::install();

    let mut terminal = setup_terminal()?;
    debug!("terminal setup complete");

    // Set up the event channel and agent manager.
    const EVENT_CHANNEL_CAPACITY: usize = 10_000;
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(EVENT_CHANNEL_CAPACITY);
    let mut manager = AgentManager::new(event_tx);
    let mut app = App::new();

    // Spawn initial agents from prompts/workspaces pairs.
    for (prompt, workspace) in prompts.iter().zip(workspaces.iter()) {
        let agent_id = manager.spawn(prompt, workspace).await?;
        app.add_agent(AgentState::new(agent_id, workspace.clone(), prompt.clone()));
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

/// Main event loop.
///
/// Renders the TUI, then multiplexes between agent output events and terminal
/// keyboard input using [`tokio::select!`]. Runs until `app.should_quit` is
/// set.
async fn event_loop(
    terminal: &mut Terminal<CrosstermBackend<Stdout>>,
    app: &mut App,
    manager: &mut AgentManager,
    event_rx: &mut mpsc::Receiver<AppEvent>,
    reader: &mut EventStream,
) -> Result<()> {
    loop {
        terminal.draw(|frame| ui::render(app, frame))?;

        tokio::select! {
            Some(event) = event_rx.recv() => {
                handle_app_event(app, manager, event);
            }
            Some(result) = reader.next() => {
                match result {
                    Ok(Event::Key(key_event)) => {
                        // Only handle key press events (ignore release/repeat).
                        if key_event.kind == KeyEventKind::Press {
                            handle_input(app, manager, key_event).await;
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "terminal input read error");
                    }
                    _ => {}
                }
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
// Viewport helpers
// ---------------------------------------------------------------------------

/// Return the content viewport height (in rows) for page-based scrolling.
///
/// Queries the current terminal size via crossterm and subtracts the fixed
/// chrome: 3 rows for the tab bar and 2 rows for the status bar. Falls back
/// to a sensible default when the terminal size cannot be determined.
fn viewport_height() -> usize {
    let (_cols, rows) = terminal::size().unwrap_or((80, 24));
    // 3 rows tab bar + 2 rows status bar = 5 rows of chrome.
    (rows as usize).saturating_sub(5).max(1)
}

// ---------------------------------------------------------------------------
// Scroll helper
// ---------------------------------------------------------------------------

/// Scroll the focused agent's output by the given signed delta.
///
/// A negative `delta` scrolls toward newer output (decreases `scroll_offset`),
/// while a positive `delta` scrolls toward older output (increases
/// `scroll_offset`). Clamps the offset to the valid range.
fn scroll_by(app: &mut App, delta: isize) {
    if let Some(agent) = app.focused_agent_mut() {
        if delta < 0 {
            agent.scroll_offset = agent.scroll_offset.saturating_sub((-delta) as usize);
            if agent.scroll_offset == 0 {
                agent.has_new_output = false;
            }
        } else {
            let max_offset = agent.output.len().saturating_sub(viewport_height());
            agent.scroll_offset = (agent.scroll_offset + delta as usize).min(max_offset);
        }
    }
}

// ---------------------------------------------------------------------------
// Event handlers
// ---------------------------------------------------------------------------

/// Handle an application event from the agent event channel.
fn handle_app_event(app: &mut App, manager: &mut AgentManager, event: AppEvent) {
    match event {
        AppEvent::AgentOutput { agent_id, line } => {
            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                agent.push_line(line);
            }
        }
        AppEvent::AgentExited {
            agent_id,
            exit_code,
        } => {
            info!(agent_id, ?exit_code, "agent exited");
            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                agent.status = AgentStatus::Exited(exit_code);
            }
            manager.notify_exited(agent_id);
        }
    }
}

/// Maximum interval between the two `g` presses in a `gg` chord.
const GG_CHORD_TIMEOUT: Duration = Duration::from_secs(1);

/// Handle a keyboard input event, dispatching to the appropriate action.
async fn handle_input(app: &mut App, manager: &mut AgentManager, key: KeyEvent) {
    // Clear a pending `g` chord on any key that isn't `g` itself.
    if key.code != KeyCode::Char('g') {
        app.pending_g = None;
    }

    // Dismiss help overlay with Escape before processing other keys.
    if app.show_help && key.code == KeyCode::Esc {
        app.show_help = false;
        return;
    }

    match key.code {
        // Quit: q or Ctrl+C.
        KeyCode::Char('q') => {
            app.should_quit = true;
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }

        // Next agent: Tab or l.
        KeyCode::Tab | KeyCode::Char('l') => {
            app.next_agent();
        }

        // Previous agent: Shift+Tab or h.
        KeyCode::BackTab | KeyCode::Char('h') => {
            app.prev_agent();
        }

        // Focus agent by number (1-9).
        KeyCode::Char(c @ '1'..='9') => {
            let index = (c as usize) - ('1' as usize);
            app.focus_agent(index);
        }

        // Scroll down toward latest (decrease scroll_offset).
        KeyCode::Char('j') | KeyCode::Down => {
            scroll_by(app, -1);
        }

        // Scroll up into history (increase scroll_offset).
        KeyCode::Char('k') | KeyCode::Up => {
            scroll_by(app, 1);
        }

        // Half-page down: Ctrl+D or PageDown.
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_by(app, -((viewport_height() / 2) as isize));
        }
        KeyCode::PageDown => {
            scroll_by(app, -((viewport_height() / 2) as isize));
        }

        // Half-page up: Ctrl+U or PageUp.
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_by(app, (viewport_height() / 2) as isize);
        }
        KeyCode::PageUp => {
            scroll_by(app, (viewport_height() / 2) as isize);
        }

        // Full-page down: Ctrl+F.
        KeyCode::Char('f') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_by(app, -(viewport_height() as isize));
        }

        // Full-page up: Ctrl+B.
        KeyCode::Char('b') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            scroll_by(app, viewport_height() as isize);
        }

        // Jump to bottom (latest output).
        KeyCode::Char('G') => {
            if let Some(agent) = app.focused_agent_mut() {
                agent.scroll_offset = 0;
                agent.has_new_output = false;
            }
        }

        // Scroll to top (oldest output) — vim-style `gg` chord.
        KeyCode::Char('g') => {
            if app
                .pending_g
                .is_some_and(|t| t.elapsed() < GG_CHORD_TIMEOUT)
            {
                // Second `g` within timeout — complete the chord.
                app.pending_g = None;
                if let Some(agent) = app.focused_agent_mut() {
                    let max_offset = agent.output.len().saturating_sub(viewport_height());
                    agent.scroll_offset = max_offset;
                }
            } else {
                // First `g` — start the chord.
                app.pending_g = Some(Instant::now());
            }
        }

        // Kill focused agent.
        //
        // NOTE: `app.focused_agent()` borrows `app` immutably and
        // `manager.kill()` borrows `manager` mutably — these are
        // independent objects so there is no conflict. We copy
        // `agent_id` out before calling `kill()` to avoid holding
        // the immutable `app` reference across the mutable
        // `manager` call. If `kill()` ever needs an `&AgentState`
        // parameter this pattern must be revisited.
        KeyCode::Char('x') => {
            if let Some(agent) = app.focused_agent() {
                let agent_id = agent.id;
                match manager.kill(agent_id).await {
                    Ok(()) => {
                        app.set_status_message(format!(
                            "Sent kill signal to agent {}",
                            agent_id + 1
                        ));
                    }
                    Err(e) => {
                        app.set_status_message(format!("Kill failed: {e}"));
                    }
                }
            }
        }

        // Toggle compact tool result display.
        KeyCode::Char('r') => {
            app.compact_results = !app.compact_results;
            let mode = if app.compact_results {
                "compact"
            } else {
                "full"
            };
            app.set_status_message(format!("Tool results: {mode}"));
        }

        // Toggle help overlay.
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }

        // All other keys: no-op.
        _ => {}
    }
}
