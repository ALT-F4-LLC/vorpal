//! Keyboard input handling for the agent TUI.
//!
//! Contains all input-dispatch logic for the three input modes: normal
//! browsing, prompt input, and confirm-close. The single public entry
//! point [`handle_key`] is called from the event loop in `tui.rs`.

use super::manager::AgentManager;
use super::state::{
    AgentActivity, AgentStatus, App, DisplayLine, InputField, InputMode, InputOverrides,
    EFFORT_LEVELS, MODELS, PERMISSION_MODES,
};
use super::ui::{BLOCK_MARKER, RESULT_CONNECTOR, SESSION_MARKER};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers},
    terminal,
};
use path_clean::PathClean;
use std::time::{Duration, Instant};
use tracing::warn;

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
// Public entry point
// ---------------------------------------------------------------------------

/// Maximum interval between the two `g` presses in a `gg` chord.
const GG_CHORD_TIMEOUT: Duration = Duration::from_secs(1);

/// Handle a keyboard input event, dispatching to the appropriate handler
/// based on the current input mode.
///
/// This is the single entry point called from the event loop in `tui.rs`.
pub async fn handle_key(app: &mut App, manager: &mut AgentManager, key: KeyEvent) {
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

        // Cycle color theme: dark → light → dark.
        KeyCode::Char('t') => {
            app.cycle_theme();
            app.set_status_message(format!("Theme: {}", app.theme.name));
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
                        let agent_opts = agent.claude_options.clone();
                        let agent_workspace = agent.workspace.clone();
                        app.respond_target = Some(agent.id);
                        app.enter_input_mode_with(Some(InputOverrides {
                            claude_options: agent_opts,
                            workspace: agent_workspace,
                        }));
                    }
                    (AgentStatus::Running, _) => {
                        app.set_status_message("Agent is still running");
                    }
                    (_, None) => {
                        app.set_status_message("No session ID -- agent must complete first");
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

                if let Some(target_id) = respond_target {
                    // Respond flow: resume an existing agent's session.
                    // Resolve the agent id to a Vec index at submit time via the
                    // agent_index map. This is correct even if agents were added
                    // or removed while the input overlay was open.
                    let session_id = app
                        .agent_by_id_mut(target_id)
                        .and_then(|a| a.session_id.clone());
                    let session_ref = session_id.as_deref();
                    // The old agent_id was already removed from the manager's tracking
                    // when notify_exited() fired. spawn() registers the new agent_id
                    // in kill_senders and handles, so no explicit cleanup is needed here.
                    match manager
                        .spawn(&prompt, &workspace, &claude_options, session_ref)
                        .await
                    {
                        Ok(agent_id) => {
                            if let Some(agent) = app.agent_by_id_mut(target_id) {
                                // Preserve existing output; add a visual separator.
                                agent.push_line(DisplayLine::TurnStart);
                                agent.id = agent_id;
                                agent.status = AgentStatus::Running;
                                agent.activity = AgentActivity::Idle;
                                agent.prompt = prompt.clone();
                                agent.scroll_offset = 0;
                                agent.has_new_output = false;
                                agent.cached_lines = None;
                                // Rebuild agent_index since the id changed.
                                app.rebuild_agent_index();
                            }
                            // Resolve the new agent_id to a Vec index for focusing.
                            if let Some(idx) = app.agent_vec_index(agent_id) {
                                app.focus_agent(idx);
                                app.set_status_message(format!("Resumed agent {}", idx + 1));
                            } else {
                                tracing::warn!(
                                    agent_id,
                                    "resumed agent but could not resolve Vec index"
                                );
                                app.set_status_message("Resumed agent (index not found)");
                            }
                        }
                        Err(e) => {
                            app.set_status_message(format!("Spawn failed: {e}"));
                        }
                    }
                } else {
                    // New-agent flow (unchanged).
                    match manager
                        .spawn(&prompt, &workspace, &claude_options, None)
                        .await
                    {
                        Ok(agent_id) => {
                            app.add_agent(super::state::AgentState::new(
                                agent_id,
                                workspace,
                                prompt.clone(),
                                claude_options.clone(),
                            ));
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
                app.input.insert_char('\n');
            }
            _ => {
                app.next_input_field();
            }
        },

        // All text editing operations route to the active field's buffer.
        code => {
            // Selector fields (PermissionMode, Model, Effort) use Left/Right
            // to cycle through predefined options instead of free-text editing.
            let selector: Option<(&[&str], &mut super::state::InputBuffer)> = match app.input_field
            {
                InputField::PermissionMode => Some((PERMISSION_MODES, &mut app.permission_mode)),
                InputField::Model => Some((MODELS, &mut app.model)),
                InputField::Effort => Some((EFFORT_LEVELS, &mut app.effort)),
                _ => None,
            };
            if let Some((options, buf)) = selector {
                let current_idx = options.iter().position(|m| *m == buf.text());
                match code {
                    KeyCode::Left => {
                        let new_idx = match current_idx {
                            Some(0) | None => options.len() - 1,
                            Some(i) => i - 1,
                        };
                        buf.set_text(options[new_idx]);
                    }
                    KeyCode::Right => {
                        let new_idx = match current_idx {
                            Some(i) if i + 1 < options.len() => i + 1,
                            _ => 0,
                        };
                        buf.set_text(options[new_idx]);
                    }
                    _ => {}
                }
                return;
            }

            let buf = match app.input_field {
                InputField::Prompt => &mut app.input,
                InputField::Workspace => &mut app.workspace,
                InputField::PermissionMode => unreachable!("handled by selector above"),
                InputField::Model => unreachable!("handled by selector above"),
                InputField::Effort => unreachable!("handled by selector above"),
                InputField::MaxBudgetUsd => &mut app.max_budget,
                InputField::AllowedTools => &mut app.allowed_tools,
                InputField::AddDir => &mut app.add_dir,
            };
            match code {
                KeyCode::Backspace => buf.delete_char(),
                KeyCode::Delete => buf.delete_char_forward(),
                KeyCode::Left => buf.move_left(),
                KeyCode::Right => buf.move_right(),
                KeyCode::Home => buf.move_home(),
                KeyCode::End => buf.move_end(),
                KeyCode::Char(c) => buf.insert_char(c),
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
async fn copy_to_clipboard(text: &str) -> anyhow::Result<()> {
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
