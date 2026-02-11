//! TUI event loop and terminal management.
//!
//! Sets up the terminal (raw mode, alternate screen), runs the main event loop
//! dispatching input and agent events, and ensures proper terminal teardown on
//! exit or panic.

use super::manager::{AgentManager, ClaudeOptions};
use super::state::{
    AgentActivity, AgentState, AgentStatus, App, AppEvent, DisplayLine, InputField, InputMode,
    EFFORT_LEVELS, MODELS, PERMISSION_MODES,
};
use super::ui;
use anyhow::Result;
use crossterm::{
    event::{Event, EventStream, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{self, EnterAlternateScreen, LeaveAlternateScreen},
};
use futures_lite::StreamExt;
use path_clean::PathClean;
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
    let (event_tx, mut event_rx) = mpsc::channel::<AppEvent>(EVENT_CHANNEL_CAPACITY);
    let mut manager = AgentManager::new(event_tx);
    let mut app = App::new(
        std::env::current_dir().unwrap_or_default(),
        claude_options.clone(),
    );

    // Spawn initial agents from prompts/workspaces pairs.
    for (prompt, workspace) in prompts.iter().zip(workspaces.iter()) {
        let agent_id = manager.spawn(prompt, workspace, &claude_options, None).await?;
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
    let mut tick_interval = tokio::time::interval(Duration::from_millis(100));

    loop {
        terminal.draw(|frame| ui::render(app, frame))?;

        tokio::select! {
            _ = tick_interval.tick() => {
                app.tick = app.tick.wrapping_add(1);
            }
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
        AppEvent::AgentExited {
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
        AppEvent::AgentSessionId { agent_id, session_id } => {
            if let Some(agent) = app.agent_by_id_mut(agent_id) {
                agent.session_id = Some(session_id);
            }
        }

    }
}

/// Maximum interval between the two `g` presses in a `gg` chord.
const GG_CHORD_TIMEOUT: Duration = Duration::from_secs(1);

/// Handle a keyboard input event, dispatching to the appropriate action.
async fn handle_input(app: &mut App, manager: &mut AgentManager, key: KeyEvent) {
    // In input mode, delegate all keys to the input handler.
    if app.input_mode == InputMode::Input {
        handle_input_mode(app, manager, key).await;
        return;
    }

    // In confirm-close mode, intercept y/n/Esc before normal handling.
    if app.confirm_close {
        handle_confirm_close(app, manager, key).await;
        return;
    }

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
        // Close focused agent tab: q.
        //
        // If the focused agent is still running, show a confirmation dialog
        // before killing it. If the agent has already exited, close the tab
        // immediately. When no agents remain the TUI exits.
        KeyCode::Char('q') => {
            if let Some(agent) = app.focused_agent() {
                match agent.status {
                    AgentStatus::Running => {
                        app.confirm_close = true;
                    }
                    AgentStatus::Exited(_) => {
                        app.remove_agent(app.focused);
                        if app.agents.is_empty() {
                            app.should_quit = true;
                        }
                    }
                }
            } else {
                // No agents — nothing to do, just quit.
                app.should_quit = true;
            }
        }

        // Force quit all agents: Ctrl+C.
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

        // Cycle tool result display mode: compact → hidden → full.
        KeyCode::Char('r') => {
            app.result_display = app.result_display.next();
            app.set_status_message(format!("Tool results: {}", app.result_display.label()));
        }

        // Copy focused agent output to system clipboard.
        //
        // Uses the cached rendered lines (which respect the current
        // ResultDisplay mode — compact/hidden/full) so the clipboard
        // matches exactly what the user sees on screen.
        KeyCode::Char('y') => {
            if let Some(agent) = app.focused_agent() {
                let text = if let Some(cached) = &agent.cached_lines {
                    cached
                        .iter()
                        .map(|line| {
                            line.spans
                                .iter()
                                .map(|span| span.content.as_ref())
                                .collect::<String>()
                        })
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    // Fallback: no cached lines yet (shouldn't happen in
                    // practice since rendering runs before input).
                    agent
                        .output
                        .iter()
                        .map(display_line_to_text)
                        .collect::<Vec<_>>()
                        .join("\n")
                };
                match copy_to_clipboard(&text).await {
                    Ok(()) => {
                        let bytes = text.len();
                        let size = if bytes >= 1024 {
                            format!("{:.1} KB", bytes as f64 / 1024.0)
                        } else {
                            format!("{bytes} bytes")
                        };
                        app.set_status_message(format!("Copied {size} to clipboard"));
                    }
                    Err(e) => {
                        app.set_status_message(format!("Copy failed: {e}"));
                    }
                }
            }
        }

        // New agent: enter prompt input mode.
        KeyCode::Char('n') => {
            app.enter_input_mode();
        }

        // Toggle help overlay.
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }

        // Respond to an exited agent (resume with session ID).
        KeyCode::Char('s') => {
            if let Some(agent) = app.focused_agent() {
                match (&agent.status, &agent.session_id) {
                    (AgentStatus::Exited(_), Some(_)) => {
                        app.respond_target = Some(app.focused);
                        app.enter_input_mode();
                    }
                    (AgentStatus::Running, _) => {
                        app.set_status_message("Agent is still running");
                    }
                    (_, None) => {
                        app.set_status_message(
                            "No session ID -- agent must complete first",
                        );
                    }
                }
            }
        }

        // All other keys: no-op.
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Input mode handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while in prompt input mode.
///
/// Supports text entry, cursor movement, backspace/delete, field switching
/// (Tab), submission (Enter), and cancellation (Escape). On submit, spawns a
/// new agent with the entered prompt and workspace directory.
async fn handle_input_mode(app: &mut App, manager: &mut AgentManager, key: KeyEvent) {
    match key.code {
        // Cancel input: Escape.
        KeyCode::Esc => {
            app.exit_input_mode();
        }

        // Cycle input fields forward (Tab) or backward (Shift+Tab).
        KeyCode::Tab => {
            app.next_input_field();
        }
        KeyCode::BackTab => {
            app.prev_input_field();
        }

        // Ctrl+S: submit the form and spawn a new agent (or respond to an
        // existing one if `respond_target` is set).
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let respond_target = app.respond_target;
            if let Some((prompt, workspace, claude_options)) = app.submit_input() {
                // Resolve relative workspace paths to absolute.
                let workspace = if workspace.is_absolute() {
                    workspace.clean()
                } else {
                    std::env::current_dir()
                        .unwrap_or_default()
                        .join(&workspace)
                        .clean()
                };

                if let Some(target_idx) = respond_target {
                    // Respond flow: resume an existing agent's session.
                    let session_id = app.agents.get(target_idx)
                        .and_then(|a| a.session_id.clone());
                    let session_ref = session_id.as_deref();
                    match manager.spawn(&prompt, &workspace, &claude_options, session_ref).await {
                        Ok(agent_id) => {
                            if let Some(agent) = app.agents.get_mut(target_idx) {
                                // Preserve existing output; add a visual separator.
                                agent.push_line(DisplayLine::TurnStart);
                                agent.id = agent_id;
                                agent.status = AgentStatus::Running;
                                agent.activity = AgentActivity::Idle;
                                agent.prompt = prompt.clone();
                                agent.scroll_offset = 0;
                                agent.has_new_output = false;
                                // Rebuild agent_index since the id changed.
                                app.rebuild_agent_index();
                            }
                            app.focus_agent(target_idx);
                            app.set_status_message(format!(
                                "Resumed agent {} (session)",
                                target_idx + 1
                            ));
                        }
                        Err(e) => {
                            app.set_status_message(format!("Spawn failed: {e}"));
                        }
                    }
                } else {
                    // New-agent flow (unchanged).
                    match manager.spawn(&prompt, &workspace, &claude_options, None).await {
                        Ok(agent_id) => {
                            app.add_agent(AgentState::new(agent_id, workspace, prompt.clone()));
                            // Focus the newly added agent.
                            let new_index = app.agents.len() - 1;
                            app.focus_agent(new_index);
                            app.set_status_message(format!("Spawned agent {}", agent_id + 1));
                        }
                        Err(e) => {
                            app.set_status_message(format!("Spawn failed: {e}"));
                        }
                    }
                }
            }
        }

        // Enter key behaviour depends on the active field:
        //   - Prompt: insert a newline (multiline prompt editing).
        //   - Any other field: advance to the next field (same as Tab).
        KeyCode::Enter => match app.input_field {
            InputField::Prompt => {
                app.input_buffer.insert(app.input_cursor, '\n');
                app.input_cursor += '\n'.len_utf8();
            }
            _ => {
                app.next_input_field();
            }
        },

        // All text editing operations route to the active field's buffer.
        code => {
            // Selector fields (PermissionMode, Model, Effort) use Left/Right
            // to cycle through predefined options instead of free-text editing.
            let selector: Option<(&[&str], &mut String, &mut usize)> = match app.input_field {
                InputField::PermissionMode => Some((
                    PERMISSION_MODES,
                    &mut app.permission_mode_buffer,
                    &mut app.permission_mode_cursor,
                )),
                InputField::Model => Some((MODELS, &mut app.model_buffer, &mut app.model_cursor)),
                InputField::Effort => Some((
                    EFFORT_LEVELS,
                    &mut app.effort_buffer,
                    &mut app.effort_cursor,
                )),
                _ => None,
            };
            if let Some((options, buffer, cursor)) = selector {
                let current_idx = options.iter().position(|m| *m == buffer.as_str());
                match code {
                    KeyCode::Left => {
                        let new_idx = match current_idx {
                            Some(0) | None => options.len() - 1,
                            Some(i) => i - 1,
                        };
                        *buffer = options[new_idx].to_string();
                        *cursor = buffer.len();
                    }
                    KeyCode::Right => {
                        let new_idx = match current_idx {
                            Some(i) if i + 1 < options.len() => i + 1,
                            _ => 0,
                        };
                        *buffer = options[new_idx].to_string();
                        *cursor = buffer.len();
                    }
                    _ => {}
                }
                return;
            }

            let (buffer, cursor) = match app.input_field {
                InputField::Prompt => (&mut app.input_buffer, &mut app.input_cursor),
                InputField::Workspace => (&mut app.workspace_buffer, &mut app.workspace_cursor),
                InputField::PermissionMode => unreachable!("handled by selector above"),
                InputField::Model => unreachable!("handled by selector above"),
                InputField::Effort => unreachable!("handled by selector above"),
                InputField::MaxBudgetUsd => {
                    (&mut app.max_budget_buffer, &mut app.max_budget_cursor)
                }
                InputField::AllowedTools => {
                    (&mut app.allowed_tools_buffer, &mut app.allowed_tools_cursor)
                }
                InputField::AddDir => (&mut app.add_dir_buffer, &mut app.add_dir_cursor),
            };
            match code {
                // Delete character before cursor: Backspace.
                KeyCode::Backspace => {
                    if *cursor > 0 {
                        // Find the previous char boundary for UTF-8 safety.
                        let prev = buffer[..*cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                        buffer.remove(prev);
                        *cursor = prev;
                    }
                }

                // Delete character at cursor: Delete.
                KeyCode::Delete => {
                    if *cursor < buffer.len() {
                        buffer.remove(*cursor);
                    }
                }

                // Move cursor left (by one char, UTF-8 safe).
                KeyCode::Left => {
                    if *cursor > 0 {
                        *cursor = buffer[..*cursor]
                            .char_indices()
                            .next_back()
                            .map(|(i, _)| i)
                            .unwrap_or(0);
                    }
                }

                // Move cursor right (by one char, UTF-8 safe).
                KeyCode::Right => {
                    if *cursor < buffer.len() {
                        let cur = *cursor;
                        let len = buffer.len();
                        *cursor = buffer[cur..]
                            .char_indices()
                            .nth(1)
                            .map(|(i, _)| cur + i)
                            .unwrap_or(len);
                    }
                }

                // Move cursor to start of line.
                KeyCode::Home => {
                    *cursor = 0;
                }

                // Move cursor to end of line.
                KeyCode::End => {
                    *cursor = buffer.len();
                }

                // Insert character at cursor position.
                KeyCode::Char(c) => {
                    buffer.insert(*cursor, c);
                    *cursor += c.len_utf8();
                }

                // All other keys: no-op.
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Confirm-close handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while the confirm-close dialog is visible.
///
/// - `y` / `Y` — confirm: kill the focused agent, remove its tab, and dismiss
///   the dialog. If the kill fails the tab is kept and an error is shown.
///   If no agents remain the TUI exits.
/// - Any other key — cancel and dismiss the dialog.
async fn handle_confirm_close(app: &mut App, manager: &mut AgentManager, key: KeyEvent) {
    match key.code {
        KeyCode::Char('y') | KeyCode::Char('Y') => {
            let mut should_remove = true;
            if let Some(agent) = app.focused_agent() {
                let agent_id = agent.id;
                if matches!(agent.status, AgentStatus::Running) {
                    if let Err(e) = manager.kill(agent_id).await {
                        warn!(agent_id, error = %e, "failed to kill agent during close");
                        app.set_status_message(format!("Kill failed: {e}"));
                        should_remove = false;
                    }
                }
            }
            app.confirm_close = false;
            if should_remove {
                let idx = app.focused;
                app.remove_agent(idx);
                if app.agents.is_empty() {
                    app.should_quit = true;
                }
            }
        }
        _ => {
            app.confirm_close = false;
        }
    }
}

// ---------------------------------------------------------------------------
// Clipboard
// ---------------------------------------------------------------------------

use super::ui::{BLOCK_MARKER, RESULT_CONNECTOR, SESSION_MARKER};

/// Convert a [`DisplayLine`] to plain text for clipboard export.
fn display_line_to_text(line: &DisplayLine) -> String {
    match line {
        DisplayLine::Text(s)
        | DisplayLine::System(s)
        | DisplayLine::Stderr(s)
        | DisplayLine::Error(s) => s.clone(),
        DisplayLine::Thinking(s) => format!("{BLOCK_MARKER} {s}"),
        DisplayLine::Result(s) => format!("{SESSION_MARKER} {s}"),
        DisplayLine::ToolUse {
            tool,
            input_preview,
        } => format!("{BLOCK_MARKER} {tool}({input_preview})"),
        DisplayLine::ToolResult { content, is_error } => {
            let prefix_first = if *is_error {
                format!("  {RESULT_CONNECTOR}  [ERROR] ")
            } else {
                format!("  {RESULT_CONNECTOR}  ")
            };
            content
                .lines()
                .enumerate()
                .map(|(i, line)| {
                    if i == 0 {
                        format!("{prefix_first}{line}")
                    } else {
                        format!("     {line}")
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        }
        DisplayLine::TurnStart => String::new(),
    }
}

/// Copy text to the system clipboard using platform-specific commands.
async fn copy_to_clipboard(text: &str) -> Result<()> {
    use std::process::Stdio;
    use tokio::io::AsyncWriteExt;
    use tokio::process::Command;

    let mut child = if cfg!(target_os = "macos") {
        Command::new("pbcopy")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?
    } else {
        Command::new("xclip")
            .args(["-selection", "clipboard"])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .or_else(|_| {
                Command::new("xsel")
                    .arg("--clipboard")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn()
            })?
    };

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(text.as_bytes()).await?;
    }

    let status = child.wait().await?;
    if status.success() {
        Ok(())
    } else {
        anyhow::bail!("clipboard command exited with {status}")
    }
}
