//! Keyboard and mouse input handling for the agent TUI.
//!
//! Contains all input-dispatch logic for the input modes: normal browsing,
//! prompt input, search, command palette, history search, and confirm-close.
//! The public entry points [`handle_key`] and [`handle_mouse`] are called
//! from the event loop in `tui.rs`.

use super::manager::AgentManager;
use super::state::{
    AgentActivity, AgentStatus, App, DiffLine, DisplayLine, InputField, InputMode, InputOverrides,
    SplitPane, EFFORT_LEVELS, MODELS, PERMISSION_MODES,
};
use super::ui::{BLOCK_MARKER, RESULT_CONNECTOR, SESSION_MARKER};
use crossterm::{
    event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind},
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
pub(super) fn viewport_height() -> usize {
    let (_cols, rows) = terminal::size().unwrap_or((80, 24));
    // 3 rows tab bar + 2 rows status bar = 5 rows of chrome.
    (rows as usize).saturating_sub(5).max(1)
}

// ---------------------------------------------------------------------------
// Scroll helper
// ---------------------------------------------------------------------------

/// Scroll the active pane's agent output by the given signed delta.
///
/// A negative `delta` scrolls toward newer output (decreases `scroll_offset`),
/// while a positive `delta` scrolls toward older output (increases
/// `scroll_offset`). Clamps the offset to the valid range.
///
/// In split-pane mode, scrolls the agent in the focused pane rather than
/// always scrolling the primary focused agent.
fn scroll_by(app: &mut App, delta: isize) {
    if let Some(agent) = app.active_agent_mut() {
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
// Shared actions (used by both keybindings and command palette)
// ---------------------------------------------------------------------------

/// Kill the focused agent (shared by `x` key and `:kill` command).
async fn action_kill(app: &mut App, manager: &mut AgentManager) {
    if let Some(agent) = app.focused_agent() {
        let agent_id = agent.id;
        match manager.kill(agent_id).await {
            Ok(()) => {
                app.set_status_message(format!("Sent kill signal to agent {}", agent_id + 1));
            }
            Err(e) => {
                app.set_status_message(format!("Kill failed: {e}"));
            }
        }
    } else {
        app.set_status_message("No agent to kill");
    }
}

/// Close the focused agent tab (shared by `q` key and `:quit` command).
///
/// If the focused agent is still running, shows a confirmation dialog.
/// If the agent has exited, removes the tab immediately. When no agents
/// remain the TUI exits.
fn action_quit_tab(app: &mut App) {
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
        app.should_quit = true;
    }
}

/// Open the settings form pre-populated with the focused agent's Claude
/// options (`:edit` command). Only exposes settings fields — the prompt and
/// workspace cannot be changed from here.
fn action_edit(app: &mut App) {
    if let Some(agent) = app.focused_agent() {
        match (&agent.status, &agent.session_id) {
            (AgentStatus::Exited(_), Some(_)) => {
                app.enter_settings_mode();
            }
            (AgentStatus::Running, _) => {
                app.set_status_message("Agent is still running");
            }
            (_, None) => {
                app.set_status_message("No session ID -- agent must complete first");
            }
        }
    } else {
        app.set_status_message("No agent to edit");
    }
}

/// Enter search mode for the focused agent (shared by `/` key and `:search` command).
fn action_search(app: &mut App) {
    if app.focused_agent().is_some() {
        app.enter_search_mode();
    } else {
        app.set_status_message("No agent to search");
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
    // Dismiss any active toast notifications on any keypress.
    if !app.toasts.is_empty() {
        app.dismiss_toasts();
    }

    // In search mode, delegate all keys to the search handler.
    if app.input_mode == InputMode::Search {
        handle_search_mode(app, key);
        return;
    }

    // In history search mode, delegate all keys to the history search handler.
    if app.input_mode == InputMode::HistorySearch {
        handle_history_search_mode(app, key);
        return;
    }

    // In command mode, delegate all keys to the command handler.
    if app.input_mode == InputMode::Command {
        handle_command_mode(app, manager, key).await;
        return;
    }

    // In template picker mode, delegate all keys to the template handler.
    if app.input_mode == InputMode::TemplatePicker {
        handle_template_mode(app, key);
        return;
    }

    // In save-template mode, delegate all keys to the save-template handler.
    if app.input_mode == InputMode::SaveTemplate {
        handle_save_template_mode(app, key);
        return;
    }

    // In inline chat mode, delegate all keys to the chat handler.
    if app.input_mode == InputMode::Chat {
        handle_chat_mode(app, manager, key).await;
        return;
    }

    // In settings mode, delegate to the settings handler.
    if app.input_mode == InputMode::Settings {
        handle_settings_mode(app, key);
        return;
    }

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

    // Dismiss dependency graph overlay with Escape before processing other keys.
    if app.show_graph && key.code == KeyCode::Esc {
        app.show_graph = false;
        return;
    }

    // Dismiss dashboard overlay with Escape before processing other keys.
    if app.show_dashboard && key.code == KeyCode::Esc {
        app.show_dashboard = false;
        return;
    }

    // When the dashboard overlay is visible, intercept navigation keys.
    if app.show_dashboard && !app.agents.is_empty() {
        match key.code {
            KeyCode::Char('j') | KeyCode::Down => {
                app.dashboard_selected = (app.dashboard_selected + 1) % app.agents.len();
                return;
            }
            KeyCode::Char('k') | KeyCode::Up => {
                app.dashboard_selected = if app.dashboard_selected == 0 {
                    app.agents.len() - 1
                } else {
                    app.dashboard_selected - 1
                };
                return;
            }
            KeyCode::Enter => {
                let idx = app.dashboard_selected;
                app.focus_agent(idx);
                app.show_dashboard = false;
                app.set_status_message(format!("Focused agent {}", idx + 1));
                return;
            }
            _ => {}
        }
    }

    // Clear search highlights with Escape when not in any overlay.
    if key.code == KeyCode::Esc && !app.search_matches.is_empty() {
        app.search_matches.clear();
        app.search_query.clear();
        app.search_match_index = 0;
        return;
    }

    match key.code {
        // Close focused agent tab: q.
        KeyCode::Char('q') => {
            action_quit_tab(app);
        }

        // Force quit all agents: Ctrl+C.
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.should_quit = true;
        }

        // Next agent: Tab or l.
        KeyCode::Tab | KeyCode::Char('l') => {
            app.next_agent();
            // Keep sidebar selection in sync with focused agent.
            if app.sidebar_visible {
                app.sidebar_selected = app.focused;
            }
        }

        // Previous agent: Shift+Tab or h.
        KeyCode::BackTab | KeyCode::Char('h') => {
            app.prev_agent();
            // Keep sidebar selection in sync with focused agent.
            if app.sidebar_visible {
                app.sidebar_selected = app.focused;
            }
        }

        // Focus agent by number (1-9).
        KeyCode::Char(c @ '1'..='9') => {
            let index = (c as usize) - ('1' as usize);
            app.focus_agent(index);
            // Keep sidebar selection in sync with focused agent.
            if app.sidebar_visible {
                app.sidebar_selected = app.focused;
            }
        }

        // Sidebar navigation: J moves selection down, K moves selection up.
        // Enter on sidebar selection focuses that agent.
        KeyCode::Char('J') if app.sidebar_visible => {
            if !app.agents.is_empty() {
                app.sidebar_selected = (app.sidebar_selected + 1) % app.agents.len();
            }
        }
        KeyCode::Char('K') if app.sidebar_visible => {
            if !app.agents.is_empty() {
                app.sidebar_selected = if app.sidebar_selected == 0 {
                    app.agents.len() - 1
                } else {
                    app.sidebar_selected - 1
                };
            }
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
            if let Some(agent) = app.active_agent_mut() {
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
                if let Some(agent) = app.active_agent_mut() {
                    let max_offset = agent.output.len().saturating_sub(viewport_height());
                    agent.scroll_offset = max_offset;
                }
            } else {
                // First `g` — start the chord.
                app.pending_g = Some(Instant::now());
            }
        }

        // Kill focused agent: x.
        KeyCode::Char('x') => {
            action_kill(app, manager).await;
        }

        // Cycle tool result display mode: compact → hidden → full.
        // Also clears all per-section overrides so every section resets
        // to the new global mode.
        KeyCode::Char('s') => {
            app.result_display = app.result_display.next();
            // Clear per-section overrides so all sections follow the new global mode.
            for agent in &mut app.agents {
                agent.clear_section_overrides();
            }
            app.set_status_message(format!("Tool results: {}", app.result_display.label()));
        }

        // Cycle color theme: dark → light → dark.
        KeyCode::Char('t') => {
            app.cycle_theme();
            app.set_status_message(format!("Theme: {}", app.theme.name));
        }

        // Copy the active pane's agent output to system clipboard.
        //
        // Uses the cached rendered lines (which respect the current
        // ResultDisplay mode — compact/hidden/full) so the clipboard
        // matches exactly what the user sees on screen.
        KeyCode::Char('y') => {
            let active_idx = app.active_agent_index();
            if let Some(agent) = active_idx.and_then(|i| app.agents.get(i)) {
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

        // Export the focused agent's session to a Markdown file.
        KeyCode::Char('e') => {
            if let Some(agent) = app.focused_agent() {
                match super::export::export_session(agent) {
                    Ok(path) => {
                        app.set_status_message(format!("Exported to {}", path.display()));
                    }
                    Err(e) => {
                        app.set_status_message(format!("Export failed: {e}"));
                    }
                }
            } else {
                app.set_status_message("No agent to export");
            }
        }

        // Next search match / new agent.
        // When search matches are active, `n` jumps to the next match.
        // Otherwise, `n` opens the quick prompt input form directly.
        KeyCode::Char('n') => {
            if !app.search_matches.is_empty() {
                app.search_next(viewport_height());
            } else {
                app.enter_input_mode();
            }
        }

        // Previous search match (Shift+N).
        KeyCode::Char('N') => {
            if !app.search_matches.is_empty() {
                app.search_prev(viewport_height());
            }
        }

        // Toggle split-pane mode: |.
        KeyCode::Char('|') => {
            if app.agents.len() >= 2 {
                app.split_enabled = !app.split_enabled;
                if app.split_enabled {
                    app.split_focused_pane = SplitPane::Left;
                    // Default right pane to the next agent after focused.
                    app.split_right_index = None;
                    app.set_status_message("Split-pane enabled");
                } else {
                    app.split_right_index = None;
                    app.split_focused_pane = SplitPane::Left;
                    app.set_status_message("Split-pane disabled");
                }
            } else {
                app.set_status_message("Need at least 2 agents for split view");
            }
        }

        // Switch focus between split panes: `.
        KeyCode::Char('`') if app.split_enabled => {
            app.split_focused_pane = match app.split_focused_pane {
                SplitPane::Left => SplitPane::Right,
                SplitPane::Right => SplitPane::Left,
            };
            let pane_label = match app.split_focused_pane {
                SplitPane::Left => "left",
                SplitPane::Right => "right",
            };
            app.set_status_message(format!("Focus: {pane_label} pane"));
        }

        // Toggle sidebar panel: b.
        KeyCode::Char('b') => {
            app.sidebar_visible = !app.sidebar_visible;
            // Sync sidebar selection with current focus when opening.
            if app.sidebar_visible {
                app.sidebar_selected = app.focused;
            }
        }

        // Toggle dependency graph overlay: v.
        KeyCode::Char('v') => {
            app.show_graph = !app.show_graph;
            if app.show_graph {
                app.show_dashboard = false;
                app.set_status_message("Dependency graph visible");
            } else {
                app.set_status_message("Dependency graph hidden");
            }
        }

        // Toggle aggregate dashboard overlay: D (Shift+d).
        KeyCode::Char('D') => {
            app.show_dashboard = !app.show_dashboard;
            if app.show_dashboard {
                app.show_graph = false;
                app.dashboard_selected = app.focused.min(app.agents.len().saturating_sub(1));
                app.set_status_message("Dashboard visible");
            } else {
                app.set_status_message("Dashboard hidden");
            }
        }

        // Toggle help overlay.
        KeyCode::Char('?') => {
            app.show_help = !app.show_help;
        }

        // Enter search mode: /.
        KeyCode::Char('/') => {
            action_search(app);
        }

        // Enter command mode: :.
        KeyCode::Char(':') => {
            app.enter_command_mode();
        }

        // Activate inline chat input: i (when agents exist and not running).
        KeyCode::Char('i') if !app.agents.is_empty() => {
            app.input_mode = InputMode::Chat;
        }

        KeyCode::Enter => {
            action_enter(app);
        }

        // All other keys: no-op.
        _ => {}
    }
}

/// Handle the Enter key in Normal mode.
///
/// Priority:
/// 1. Sidebar selection focus (when sidebar visible and selection differs)
/// 2. Tool section toggle (when a section header is at viewport top)
fn action_enter(app: &mut App) {
    if app.sidebar_visible && app.sidebar_selected != app.focused {
        app.focus_agent(app.sidebar_selected);
    } else if let Some(section_idx) = find_section_at_viewport_top(app) {
        let global = app.result_display;
        if let Some(agent) = app.focused_agent_mut() {
            agent.toggle_section(section_idx, global);
            let mode = agent
                .section_overrides
                .get(&section_idx)
                .copied()
                .unwrap_or(global);
            app.set_status_message(format!(
                "Section {}: {}",
                section_idx + 1,
                mode.label()
            ));
        }
    }
}

// ---------------------------------------------------------------------------
// Section toggle helper
// ---------------------------------------------------------------------------

/// Find the tool section index at the top of the current viewport.
///
/// Scans the focused agent's output to find the ToolUse header line whose
/// associated tool result content is visible at or near the top of the
/// viewport. Returns the section index (0-based count of ToolUse lines from
/// the start of output) suitable for use as a key into `section_overrides`.
///
/// The algorithm:
/// 1. Determine which raw output lines are visible in the viewport.
/// 2. Scan from the top of the visible region looking for a ToolUse line.
/// 3. If found, count how many ToolUse lines precede it to get the section index.
/// 4. If no ToolUse is visible at the top, scan slightly above the viewport
///    (the ToolUse header may have scrolled just out of view while its results
///    are still visible).
fn find_section_at_viewport_top(app: &App) -> Option<usize> {
    let agent = app.focused_agent()?;
    let len = agent.output.len();
    if len == 0 {
        return None;
    }

    let height = viewport_height();
    let max_offset = len.saturating_sub(height);
    let offset = agent.scroll_offset.min(max_offset);
    let end = len.saturating_sub(offset);
    let start = end.saturating_sub(height);

    // First, scan visible lines from the top for a ToolUse header.
    for idx in start..end {
        if matches!(agent.output[idx], DisplayLine::ToolUse { .. }) {
            return Some(count_tool_use_before(agent, idx));
        }
        // If we hit a non-ToolResult, non-ToolUse line after scanning a few
        // lines, stop looking forward — the top of the viewport is in the
        // middle of text content, not near a tool section.
        if !matches!(
            agent.output[idx],
            DisplayLine::ToolResult { .. } | DisplayLine::ToolUse { .. }
        ) && idx > start + 2
        {
            break;
        }
    }

    // If the top of the viewport shows ToolResult lines but the ToolUse header
    // has scrolled just above, scan backwards from `start` to find it.
    if start > 0
        && matches!(
            agent.output.get(start),
            Some(DisplayLine::ToolResult { .. })
        )
    {
        let search_from = start.saturating_sub(1);
        for idx in (0..=search_from).rev() {
            if matches!(agent.output[idx], DisplayLine::ToolUse { .. }) {
                return Some(count_tool_use_before(agent, idx));
            }
            // Stop if we hit something that isn't part of this tool section.
            if !matches!(agent.output[idx], DisplayLine::ToolResult { .. }) {
                break;
            }
        }
    }

    None
}

/// Count the number of ToolUse lines in the output before (and including)
/// the given index, returning a 0-based section index.
fn count_tool_use_before(agent: &super::state::AgentState, target_idx: usize) -> usize {
    let mut count = 0;
    for idx in 0..=target_idx {
        if matches!(agent.output[idx], DisplayLine::ToolUse { .. }) {
            if idx == target_idx {
                return count;
            }
            count += 1;
        }
    }
    count
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

        // Ctrl+E: toggle between quick-launch and advanced input modes.
        KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.quick_launch = !app.quick_launch;
            // When switching to quick mode, clamp the focused field to one
            // that's visible (Prompt or Workspace).
            if app.quick_launch
                && !matches!(app.input_field, InputField::Prompt | InputField::Workspace)
            {
                app.input_field = InputField::Prompt;
            }
            let mode = if app.quick_launch {
                "quick"
            } else {
                "advanced"
            };
            app.set_status_message(format!("Input mode: {mode}"));
        }

        // Ctrl+R: open history search overlay.
        KeyCode::Char('r') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.enter_history_search();
        }

        // Ctrl+T: save current form as a named template.
        KeyCode::Char('t') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.enter_save_template();
        }

        // Ctrl+P: open the template picker from within the input form.
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.enter_template_picker(true);
        }

        // Ctrl+S: submit the form and spawn a new agent (or respond to an
        // existing one if `respond_target` is set).
        KeyCode::Char('s') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            submit_and_spawn(app, manager).await;
        }

        // Enter key behaviour depends on the active field and mode:
        //   - Quick-launch mode: submit the form (launch immediately).
        //   - Advanced mode, Prompt field: insert a newline (multiline editing).
        //   - Advanced mode, any other field: advance to the next field (Tab).
        KeyCode::Enter => {
            if app.quick_launch {
                submit_and_spawn(app, manager).await;
            } else {
                match app.input_field {
                    InputField::Prompt => {
                        app.input.insert_char('\n');
                    }
                    _ => {
                        app.next_input_field();
                    }
                }
            }
        }

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

            // Up/Down on the Prompt field cycle through history.
            if app.input_field == InputField::Prompt {
                match code {
                    KeyCode::Up => {
                        app.history_prev();
                        return;
                    }
                    KeyCode::Down => {
                        app.history_next();
                        return;
                    }
                    _ => {}
                }
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
                KeyCode::Char(c) => {
                    // Reset history browsing when the user types a character.
                    if app.input_field == InputField::Prompt {
                        app.history_index = None;
                    }
                    buf.insert_char(c);
                }
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Submit + spawn helper
// ---------------------------------------------------------------------------

/// Submit the current input form and spawn (or resume) an agent.
///
/// Extracts the respond-target before calling `submit_input()` so the
/// borrow of `app` is released before spawning. Shared by both the
/// `Ctrl+S` and quick-launch `Enter` key paths.
pub(super) async fn submit_and_spawn(app: &mut App, manager: &mut AgentManager) {
    let respond_target = app.respond_target;
    let Some((prompt, workspace, claude_options)) = app.submit_input() else {
        app.set_status_message("Prompt cannot be empty");
        return;
    };
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
        let session_id = app
            .agent_by_id_mut(target_id)
            .and_then(|a| a.session_id.clone());
        let session_ref = session_id.as_deref();
        match manager
            .spawn(&prompt, &workspace, &claude_options, session_ref)
            .await
        {
            Ok(agent_id) => {
                // Save to history on successful spawn.
                app.save_to_history(&prompt, &workspace, &claude_options);

                if let Some(agent) = app.agent_by_id_mut(target_id) {
                    agent.push_line(DisplayLine::TurnStart);
                    agent.id = agent_id;
                    agent.status = AgentStatus::Running;
                    agent.activity = AgentActivity::Idle;
                    agent.prompt = prompt.clone();
                    agent.scroll_offset = 0;
                    agent.has_new_output = false;
                    agent.cached_lines = None;
                    agent.resume_timer();
                    app.rebuild_agent_index();
                }
                if let Some(idx) = app.agent_vec_index(agent_id) {
                    app.focus_agent(idx);
                    app.set_status_message(format!("Resumed agent {}", idx + 1));
                } else {
                    tracing::warn!(agent_id, "resumed agent but could not resolve Vec index");
                    app.set_status_message("Resumed agent (index not found)");
                }
            }
            Err(e) => {
                app.set_status_message(format!("Spawn failed: {e}"));
            }
        }
    } else {
        // New-agent flow.
        match manager
            .spawn(&prompt, &workspace, &claude_options, None)
            .await
        {
            Ok(agent_id) => {
                // Save to history on successful spawn.
                app.save_to_history(&prompt, &workspace, &claude_options);

                app.add_agent(super::state::AgentState::new(
                    agent_id,
                    String::new(),
                    workspace,
                    prompt.clone(),
                    claude_options.clone(),
                ));
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

// ---------------------------------------------------------------------------
// Settings mode handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while in settings mode.
///
/// Tab/BackTab cycles through the 6 option fields. Enter saves and exits.
/// Esc cancels without saving. Text editing keys modify the active field.
fn handle_settings_mode(app: &mut App, key: KeyEvent) {
    use InputField::*;
    let settings_fields = [PermissionMode, Model, Effort, MaxBudgetUsd, AllowedTools, AddDir];

    match key.code {
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.permission_mode.clear();
            app.model.clear();
            app.effort.clear();
            app.max_budget.clear();
            app.allowed_tools.clear();
            app.add_dir.clear();
        }
        KeyCode::Enter => {
            app.save_settings();
        }
        KeyCode::Tab => {
            if let Some(pos) = settings_fields.iter().position(|f| *f == app.input_field) {
                app.input_field = settings_fields[(pos + 1) % settings_fields.len()];
            }
        }
        KeyCode::BackTab => {
            if let Some(pos) = settings_fields.iter().position(|f| *f == app.input_field) {
                let prev = if pos == 0 { settings_fields.len() - 1 } else { pos - 1 };
                app.input_field = settings_fields[prev];
            }
        }
        _ => {
            let buf = match app.input_field {
                PermissionMode => &mut app.permission_mode,
                Model => &mut app.model,
                Effort => &mut app.effort,
                MaxBudgetUsd => &mut app.max_budget,
                AllowedTools => &mut app.allowed_tools,
                AddDir => &mut app.add_dir,
                _ => return,
            };
            match key.code {
                KeyCode::Char(c) => buf.insert_char(c),
                KeyCode::Backspace => buf.delete_char(),
                KeyCode::Delete => buf.delete_char_forward(),
                KeyCode::Left => buf.move_left(),
                KeyCode::Right => buf.move_right(),
                KeyCode::Home => buf.move_home(),
                KeyCode::End => buf.move_end(),
                _ => {}
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Inline chat mode handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while in inline chat mode.
///
/// Supports text entry, cursor movement, Shift+Enter for newlines, and Enter
/// to submit. Escape exits chat mode (preserving any text). Up/Down cycle
/// through prompt history when the buffer is empty.
async fn handle_chat_mode(app: &mut App, manager: &mut AgentManager, key: KeyEvent) {
    match key.code {
        // Exit chat mode, preserving text in the buffer.
        KeyCode::Esc => {
            app.input_mode = InputMode::Normal;
            app.chat_history_index = None;
        }

        // Enter: submit. Shift+Enter or Alt+Enter inserts a newline.
        // Shift+Enter requires the Kitty keyboard protocol (enabled in
        // setup_terminal when supported); Alt+Enter works everywhere as
        // a fallback.
        KeyCode::Enter => {
            if key.modifiers.intersects(KeyModifiers::SHIFT | KeyModifiers::ALT) {
                app.chat_input.insert_char('\n');
            } else {
                submit_chat_message(app, manager).await;
            }
        }

        // Standard text-editing keys.
        KeyCode::Backspace => app.chat_input.delete_char(),
        KeyCode::Delete => app.chat_input.delete_char_forward(),
        KeyCode::Left => app.chat_input.move_left(),
        KeyCode::Right => app.chat_input.move_right(),
        KeyCode::Home => app.chat_input.move_home(),
        KeyCode::End => app.chat_input.move_end(),

        // Up/Down: move cursor between lines in multiline text, or cycle
        // history when on the first/last line (or buffer is empty).
        KeyCode::Up => {
            if !app.chat_input.move_up() {
                app.history_prev_chat();
            }
        }
        KeyCode::Down => {
            if !app.chat_input.move_down() {
                app.history_next_chat();
            }
        }

        // Character input (with Ctrl modifiers for shortcuts).
        KeyCode::Char(c) => {
            if key.modifiers.contains(KeyModifiers::CONTROL) {
                // Ctrl+C: exit chat mode without submitting.
                if c == 'c' {
                    app.input_mode = InputMode::Normal;
                }
                // Other Ctrl combinations are no-ops in chat mode.
            } else {
                app.chat_input.insert_char(c);
            }
        }

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Inline chat submission
// ---------------------------------------------------------------------------

/// Submit the inline chat message and spawn or resume an agent.
///
/// Extracts the text from `chat_input`, validates it, then uses the
/// existing respond or new-agent flow by populating the standard input
/// fields and calling [`submit_and_spawn`].
async fn submit_chat_message(app: &mut App, manager: &mut AgentManager) {
    let text = app.chat_input.text().trim().to_string();
    if text.is_empty() {
        return;
    }

    if app.agents.is_empty() {
        app.set_status_message("No agents. Press 'n' to create one.");
        return;
    }

    let agent = match app.agents.get(app.focused) {
        Some(a) => a,
        None => {
            app.set_status_message("No agent focused");
            return;
        }
    };
    match &agent.status {
        AgentStatus::Running => {
            let agent_mut = app.agents.get_mut(app.focused).unwrap();
            if agent_mut.message_queue.len() >= super::state::MAX_MESSAGE_QUEUE {
                app.set_status_message(format!(
                    "Queue full ({} messages) — wait for agent to finish",
                    super::state::MAX_MESSAGE_QUEUE
                ));
                return;
            }
            agent_mut.message_queue.push_back(text);
            let queue_len = agent_mut.message_queue.len();
            app.chat_input.clear();
            app.chat_history_index = None;
            app.chat_history_stash.clear();
            app.input_mode = InputMode::Normal;
            app.set_status_message(format!("Message queued ({queue_len} pending)"));
            return;
        }
        AgentStatus::Exited(_) => {
            if agent.session_id.is_none() {
                app.set_status_message("No session ID — agent must complete first.");
                return;
            }
            // Set up the respond flow: populate the standard input form
            // from the focused agent's options and inject the chat text as
            // the prompt, then trigger submit_and_spawn().
            let agent_opts = agent.claude_options.clone();
            let agent_workspace = agent.workspace.clone();
            app.respond_target = Some(agent.id);
            app.enter_input_mode_with(Some(InputOverrides {
                claude_options: agent_opts,
                workspace: agent_workspace,
            }));
            // Overwrite the prompt field with our chat text.
            app.input.set_text(&text);
            // Clear the inline chat buffer and reset history state.
            app.chat_input.clear();
            app.chat_history_index = None;
            app.chat_history_stash.clear();
            // submit_and_spawn will call exit_input_mode() internally
            // (via submit_input()), restoring InputMode::Normal.
            submit_and_spawn(app, manager).await;
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
// Template picker mode handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while in template picker mode.
///
/// The picker shows a list: "Blank" at index 0, then all templates.
/// Up/Down navigate, Enter selects, Escape cancels. `d` deletes a user
/// template (built-ins cannot be deleted).
fn handle_template_mode(app: &mut App, key: KeyEvent) {
    // Total items = 1 (Blank) + template_list.len()
    let total = 1 + app.template_list.len();

    match key.code {
        // Exit template picker: Escape.
        KeyCode::Esc => {
            app.exit_template_picker();
        }

        // Select the current template or "Blank": Enter.
        KeyCode::Enter => {
            app.select_template();
        }

        // Navigate up.
        KeyCode::Up | KeyCode::Char('k') => {
            if total > 0 {
                app.template_selected = if app.template_selected == 0 {
                    total - 1
                } else {
                    app.template_selected - 1
                };
            }
        }

        // Navigate down.
        KeyCode::Down | KeyCode::Char('j') => {
            if total > 0 {
                app.template_selected = (app.template_selected + 1) % total;
            }
        }

        // Delete user template: d (only for non-built-in templates).
        KeyCode::Char('d') => {
            if app.template_selected > 0 {
                let idx = app.template_selected - 1;
                if let Some(tmpl) = app.template_list.get(idx) {
                    if tmpl.builtin {
                        app.set_status_message("Cannot delete built-in template");
                    } else {
                        let name = tmpl.name.clone();
                        app.templates.delete_template(&name);
                        app.template_list = app.templates.all_templates_owned();
                        // Clamp selection.
                        let new_total = 1 + app.template_list.len();
                        if app.template_selected >= new_total {
                            app.template_selected = new_total.saturating_sub(1);
                        }
                        app.set_status_message(format!("Deleted template '{name}'"));
                    }
                }
            }
        }

        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Save-template mode handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while in save-template name dialog.
///
/// The user types a template name and presses Enter to save, or Escape to cancel.
fn handle_save_template_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        // Cancel: Escape.
        KeyCode::Esc => {
            app.exit_save_template();
        }

        // Save the template: Enter.
        KeyCode::Enter => {
            app.save_current_as_template();
        }

        // Text editing for the template name.
        KeyCode::Backspace => app.template_name_input.delete_char(),
        KeyCode::Delete => app.template_name_input.delete_char_forward(),
        KeyCode::Left => app.template_name_input.move_left(),
        KeyCode::Right => app.template_name_input.move_right(),
        KeyCode::Home => app.template_name_input.move_home(),
        KeyCode::End => app.template_name_input.move_end(),
        KeyCode::Char(c) => app.template_name_input.insert_char(c),
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Search mode handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while in search mode.
///
/// Supports text entry for the search query, `n`/`N` to jump between matches,
/// `Enter` to confirm and stay in search highlighting mode, and `Escape` to
/// clear the search and return to normal mode.
fn handle_search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        // Exit search mode: Escape.
        KeyCode::Esc => {
            app.exit_search_mode();
        }

        // Confirm search and return to normal mode, keeping highlights.
        KeyCode::Enter => {
            if !app.search_matches.is_empty() {
                app.scroll_to_current_match(viewport_height());
            }
            app.input_mode = InputMode::Normal;
        }

        // Next match: n or Ctrl+N (also works while typing).
        KeyCode::Char('n') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.search_next(viewport_height());
        }

        // Previous match: Ctrl+P.
        KeyCode::Char('p') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            app.search_prev(viewport_height());
        }

        // Text editing for the search query.
        KeyCode::Backspace => {
            app.search_query.delete_char();
            app.recompute_search_matches();
        }
        KeyCode::Delete => {
            app.search_query.delete_char_forward();
            app.recompute_search_matches();
        }
        KeyCode::Left => {
            app.search_query.move_left();
        }
        KeyCode::Right => {
            app.search_query.move_right();
        }
        KeyCode::Home => {
            app.search_query.move_home();
        }
        KeyCode::End => {
            app.search_query.move_end();
        }
        KeyCode::Char(c) => {
            app.search_query.insert_char(c);
            app.recompute_search_matches();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// History search mode handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while in history search mode (Ctrl+R overlay).
///
/// Supports fuzzy-searching through prompt history. Up/Down navigate the
/// filtered results, Enter selects an entry, and Escape cancels.
fn handle_history_search_mode(app: &mut App, key: KeyEvent) {
    match key.code {
        // Exit history search: Escape.
        KeyCode::Esc => {
            app.exit_history_search();
        }

        // Select the current entry and populate the form: Enter.
        KeyCode::Enter => {
            app.select_history_search_entry();
        }

        // Navigate results: Up.
        KeyCode::Up => {
            if !app.history_search_results.is_empty() {
                app.history_search_selected = if app.history_search_selected == 0 {
                    app.history_search_results.len() - 1
                } else {
                    app.history_search_selected - 1
                };
            }
        }

        // Navigate results: Down.
        KeyCode::Down => {
            if !app.history_search_results.is_empty() {
                app.history_search_selected =
                    (app.history_search_selected + 1) % app.history_search_results.len();
            }
        }

        // Text editing for the search query.
        KeyCode::Backspace => {
            app.history_search_query.delete_char();
            app.refilter_history_search();
        }
        KeyCode::Delete => {
            app.history_search_query.delete_char_forward();
            app.refilter_history_search();
        }
        KeyCode::Left => {
            app.history_search_query.move_left();
        }
        KeyCode::Right => {
            app.history_search_query.move_right();
        }
        KeyCode::Home => {
            app.history_search_query.move_home();
        }
        KeyCode::End => {
            app.history_search_query.move_end();
        }
        KeyCode::Char(c) => {
            app.history_search_query.insert_char(c);
            app.refilter_history_search();
        }
        _ => {}
    }
}

// ---------------------------------------------------------------------------
// Command mode handler
// ---------------------------------------------------------------------------

/// Handle keyboard input while in command palette mode.
///
/// Supports text entry for the command name, arrow keys to navigate the
/// filtered command list, `Tab` for completion, `Enter` to execute, and
/// `Escape` to cancel.
async fn handle_command_mode(app: &mut App, manager: &mut AgentManager, key: KeyEvent) {
    match key.code {
        // Exit command mode: Escape.
        KeyCode::Esc => {
            app.exit_command_mode();
        }

        // Execute the selected command or typed text: Enter.
        KeyCode::Enter => {
            if app.command_filtered.is_empty() {
                // No matching command — show error.
                let input = app.command_input.text().to_string();
                app.command_error = Some(format!("Unknown command: {input}"));
            } else {
                let selected = app.command_selected.min(app.command_filtered.len() - 1);
                let cmd_name = app.command_filtered[selected].name;
                app.exit_command_mode();
                execute_command(app, manager, cmd_name).await;
            }
        }

        // Tab completion: fill in the selected command name.
        KeyCode::Tab => {
            if !app.command_filtered.is_empty() {
                let selected = app.command_selected.min(app.command_filtered.len() - 1);
                app.command_input
                    .set_text(app.command_filtered[selected].name);
                app.command_selected = 0;
                app.command_error = None;
                app.refilter_commands();
            }
        }

        // Navigate filtered list: Up.
        KeyCode::Up => {
            if !app.command_filtered.is_empty() {
                app.command_selected = if app.command_selected == 0 {
                    app.command_filtered.len() - 1
                } else {
                    app.command_selected - 1
                };
            }
        }

        // Navigate filtered list: Down.
        KeyCode::Down => {
            if !app.command_filtered.is_empty() {
                app.command_selected = (app.command_selected + 1) % app.command_filtered.len();
            }
        }

        // Text editing for the command input.
        KeyCode::Backspace => {
            app.command_input.delete_char();
            app.command_selected = 0;
            app.command_error = None;
            app.refilter_commands();
        }
        KeyCode::Delete => {
            app.command_input.delete_char_forward();
            app.command_selected = 0;
            app.command_error = None;
            app.refilter_commands();
        }
        KeyCode::Left => {
            app.command_input.move_left();
        }
        KeyCode::Right => {
            app.command_input.move_right();
        }
        KeyCode::Home => {
            app.command_input.move_home();
        }
        KeyCode::End => {
            app.command_input.move_end();
        }
        KeyCode::Char(c) => {
            app.command_input.insert_char(c);
            app.command_selected = 0;
            app.command_error = None;
            app.refilter_commands();
        }
        _ => {}
    }
}

/// Execute a named command from the palette.
async fn execute_command(app: &mut App, manager: &mut AgentManager, name: &str) {
    match name {
        "kill" => action_kill(app, manager).await,
        "new" => app.enter_input_mode(),
        "edit" => action_edit(app),
        "search" => action_search(app),
        "dashboard" => {
            app.show_dashboard = !app.show_dashboard;
            if app.show_dashboard {
                app.show_graph = false;
                app.dashboard_selected = app.focused.min(app.agents.len().saturating_sub(1));
            }
        }
        "help" => app.show_help = !app.show_help,
        "quit" => action_quit_tab(app),
        "copy" => {
            let active_idx = app.active_agent_index();
            if let Some(agent) = active_idx.and_then(|i| app.agents.get(i)) {
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
        "export" => {
            if let Some(agent) = app.focused_agent() {
                match super::export::export_session(agent) {
                    Ok(path) => {
                        app.set_status_message(format!("Exported to {}", path.display()));
                    }
                    Err(e) => {
                        app.set_status_message(format!("Export failed: {e}"));
                    }
                }
            } else {
                app.set_status_message("No agent to export");
            }
        }
        "results" => {
            app.result_display = app.result_display.next();
            for agent in &mut app.agents {
                agent.clear_section_overrides();
            }
            app.set_status_message(format!("Tool results: {}", app.result_display.label()));
        }
        "sidebar" => {
            app.sidebar_visible = !app.sidebar_visible;
            if app.sidebar_visible {
                app.sidebar_selected = app.focused;
            }
        }
        "graph" => {
            app.show_graph = !app.show_graph;
            if app.show_graph {
                app.show_dashboard = false;
                app.set_status_message("Dependency graph visible");
            } else {
                app.set_status_message("Dependency graph hidden");
            }
        }
        "split" => {
            if app.agents.len() >= 2 {
                app.split_enabled = !app.split_enabled;
                if app.split_enabled {
                    app.split_focused_pane = SplitPane::Left;
                    app.split_right_index = None;
                    app.set_status_message("Split-pane enabled");
                } else {
                    app.split_right_index = None;
                    app.split_focused_pane = SplitPane::Left;
                    app.set_status_message("Split-pane disabled");
                }
            } else {
                app.set_status_message("Need at least 2 agents for split view");
            }
        }
        "theme" => {
            app.cycle_theme();
            app.set_status_message(format!("Theme: {}", app.theme.name));
        }
        "settings" => app.enter_settings_mode(),
        "clear-queue" => {
            let cleared = app.clear_focused_queue();
            if cleared > 0 {
                app.set_status_message(format!("Cleared {cleared} queued message(s)"));
            } else {
                app.set_status_message("No messages in queue");
            }
        }
        _ => app.set_status_message(format!("Unknown command: {name}")),
    }
}

// ---------------------------------------------------------------------------
// Mouse handler
// ---------------------------------------------------------------------------

/// Handle a mouse input event. Returns `true` if the event caused a state
/// change that requires a redraw.
///
/// Supports:
/// - Left-click on tabs to switch agents
/// - Scroll wheel in the content area to scroll output
/// - Left-click on input form fields to focus them
///
/// Mouse events are ignored when [`App::mouse_enabled`] is false (checked by
/// the caller in the event loop).
pub async fn handle_mouse(app: &mut App, event: MouseEvent) -> bool {
    match event.kind {
        // Left-click: tab switching, input field focus, dismiss overlays.
        MouseEventKind::Down(MouseButton::Left) => {
            let col = event.column;
            let row = event.row;

            // Dismiss toasts on any click.
            if !app.toasts.is_empty() {
                app.dismiss_toasts();
            }

            // If in input mode, check if the click lands on an input field.
            if app.input_mode == InputMode::Input {
                for &(rect, field) in &app.input_field_rects {
                    if rect_contains(rect, col, row) {
                        app.input_field = field;
                        return true;
                    }
                }
                // Click outside the input overlay dismisses it.
                return false;
            }

            // Dismiss help overlay on click.
            if app.show_help {
                app.show_help = false;
                return true;
            }

            // Dismiss confirm-close on click outside.
            if app.confirm_close {
                app.confirm_close = false;
                return true;
            }

            // Check sidebar click regions.
            if app.sidebar_visible {
                for &(rect, agent_index) in &app.sidebar_rects {
                    if rect_contains(rect, col, row) {
                        app.focus_agent(agent_index);
                        app.sidebar_selected = agent_index;
                        return true;
                    }
                }
            }

            // Click on the inline chat input area activates chat mode.
            // Only when in Normal mode so clicks don't punch through
            // modal overlays (Input, Settings, Help, etc.).
            if app.input_mode == InputMode::Normal
                && app.chat_input_rect.height > 0
                && rect_contains(app.chat_input_rect, col, row)
                && !app.agents.is_empty()
            {
                app.input_mode = InputMode::Chat;
                return true;
            }

            // Check tab click regions.
            for &(rect, agent_index) in &app.tab_rects {
                if rect_contains(rect, col, row) {
                    app.focus_agent(agent_index);
                    if app.sidebar_visible {
                        app.sidebar_selected = agent_index;
                    }
                    return true;
                }
            }

            false
        }

        // Scroll up (toward older output).
        MouseEventKind::ScrollUp => {
            if let Some(content_rect) = app.content_rect {
                if rect_contains(content_rect, event.column, event.row) {
                    scroll_by(app, 3);
                    return true;
                }
            }
            false
        }

        // Scroll down (toward newer output).
        MouseEventKind::ScrollDown => {
            if let Some(content_rect) = app.content_rect {
                if rect_contains(content_rect, event.column, event.row) {
                    scroll_by(app, -3);
                    return true;
                }
            }
            false
        }

        // All other mouse events (drag, move, right-click, middle-click) are
        // intentionally ignored. Middle-click paste is handled by the terminal
        // emulator directly and does not generate a crossterm event, so it
        // continues to work normally.
        _ => false,
    }
}

/// Test whether a point (col, row) falls within a [`ratatui::layout::Rect`].
fn rect_contains(rect: ratatui::layout::Rect, col: u16, row: u16) -> bool {
    col >= rect.x && col < rect.x + rect.width && row >= rect.y && row < rect.y + rect.height
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
        DisplayLine::DiffResult {
            diff_ops,
            file_path,
        } => {
            let mut out = format!("  {RESULT_CONNECTOR}  diff {file_path}\n");
            for op in diff_ops {
                match op {
                    DiffLine::Equal(line) => out.push_str(&format!("       {line}\n")),
                    DiffLine::Delete(line) => out.push_str(&format!("     - {line}\n")),
                    DiffLine::Insert(line) => out.push_str(&format!("     + {line}\n")),
                }
            }
            out
        }
        DisplayLine::AgentMessage {
            sender,
            recipient,
            content,
        } => format!("[{sender} -> {recipient}] {content}"),
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
