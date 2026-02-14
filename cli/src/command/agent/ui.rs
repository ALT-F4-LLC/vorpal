//! TUI widget rendering.
//!
//! Builds the ratatui widget tree from application state: tab bar, content
//! area with scrollable output, status bar, and help overlay.

use super::state::{
    AgentActivity, AgentStatus, App, DisplayLine, InputField, InputMode, ResultDisplay, SplitPane,
    ToastKind, COMMANDS, EFFORT_LEVELS, MODELS, PERMISSION_MODES,
};
use super::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;

// ---------------------------------------------------------------------------
// Unicode constants
// ---------------------------------------------------------------------------

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ARROW_RIGHT: &str = "▶";
const CHECK_MARK: &str = "✔";
const CROSS_MARK: &str = "✘";
const CIRCLE: &str = "●";
pub(super) const BLOCK_MARKER: &str = "⏺";
pub(super) const RESULT_CONNECTOR: &str = "⎿";
pub(super) const SESSION_MARKER: &str = "✻";

/// Indentation that matches the width of `⏺ ` (marker + space).
const CONTINUATION_INDENT: &str = "  ";

/// Minimum terminal width required for the TUI.
const MIN_WIDTH: u16 = 40;
/// Minimum terminal height required for the TUI.
const MIN_HEIGHT: u16 = 8;

/// Terminal width at which the full (untruncated) hint bar is shown.
const FULL_HINTS_WIDTH: u16 = 120;

// ---------------------------------------------------------------------------
// Main render entry point
// ---------------------------------------------------------------------------

/// Render the entire TUI from application state.
///
/// Splits the terminal into three vertical sections (tab bar, content area,
/// status bar) and draws each one. If the help overlay is active it is drawn
/// on top of the full terminal area.
pub fn render(app: &mut App, frame: &mut Frame) {
    let area = frame.area();
    let theme = &app.theme;

    // Gracefully handle terminals below minimum usable size — clear and bail.
    if area.height < MIN_HEIGHT || area.width < MIN_WIDTH {
        let msg = Paragraph::new(format!(
            "Terminal too small (min {}x{})",
            MIN_WIDTH, MIN_HEIGHT
        ))
        .alignment(Alignment::Center)
        .style(Style::default().fg(theme.terminal_too_small));
        frame.render_widget(msg, area);
        return;
    }

    // When the sidebar is visible, split horizontally first: sidebar | main.
    // The sidebar width is ~30 columns, clamped so it never exceeds 40% of
    // the terminal width.
    let sidebar_width: u16 = if app.sidebar_visible {
        30.min(area.width * 2 / 5)
    } else {
        0
    };

    let h_chunks = if sidebar_width > 0 {
        Layout::horizontal([Constraint::Length(sidebar_width), Constraint::Fill(1)]).split(area)
    } else {
        // No sidebar — the main area gets the full width.
        Layout::horizontal([Constraint::Length(0), Constraint::Fill(1)]).split(area)
    };

    let sidebar_area = h_chunks[0];
    let main_area = h_chunks[1];

    let chunks = Layout::vertical([
        Constraint::Length(3), // tab bar
        Constraint::Fill(1),   // content
        Constraint::Length(2), // status bar
    ])
    .split(main_area);

    // Store the content area rect for mouse scroll hit-testing.
    app.content_rect = Some(chunks[1]);

    render_tabs(app, frame, chunks[0]);

    // In split-pane mode, divide the content area into two side-by-side panes.
    if app.split_enabled {
        if let Some(right_idx) = app.split_right_agent_index() {
            let split_chunks =
                Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                    .split(chunks[1]);
            let left_focused = app.split_focused_pane == SplitPane::Left;
            render_content_pane(app, frame, split_chunks[0], app.focused, left_focused);
            render_content_pane(app, frame, split_chunks[1], right_idx, !left_focused);
        } else {
            render_content(app, frame, chunks[1]);
        }
    } else {
        render_content(app, frame, chunks[1]);
    }

    render_status(app, frame, chunks[2]);

    if app.sidebar_visible && sidebar_width > 0 {
        render_sidebar(app, frame, sidebar_area);
    }

    if app.show_help {
        render_help(&app.theme, frame, area);
    }

    if app.confirm_close {
        render_confirm_close(app, frame, area);
    }

    if app.input_mode == InputMode::TemplatePicker {
        render_template_picker(app, frame, area);
    }

    if app.input_mode == InputMode::Input {
        render_input(app, frame, area);
    }

    if app.input_mode == InputMode::SaveTemplate {
        render_input(app, frame, area);
        render_save_template_dialog(app, frame, area);
    }

    if app.input_mode == InputMode::HistorySearch {
        render_input(app, frame, area);
        render_history_search(app, frame, area);
    }

    if app.input_mode == InputMode::Search {
        render_search_bar(app, frame, chunks[2]);
    }

    if app.input_mode == InputMode::Command {
        render_command_palette(app, frame, chunks[2]);
    }

    if !app.toasts.is_empty() {
        render_toast(app, frame, area);
    }
}

// ---------------------------------------------------------------------------
// Tab bar
// ---------------------------------------------------------------------------

/// Render the tab bar showing one tab per agent with status badges.
///
/// Also updates [`App::tab_rects`] with the click regions for each visible
/// tab so mouse input can map clicks to agent indices.
fn render_tabs(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // Clear previous tab rects.
    app.tab_rects.clear();

    if app.agents.is_empty() {
        let hint = Paragraph::new(" [No agents] — press 'n' to start one")
            .style(Style::default().fg(theme.tab_no_agents))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(theme.tab_border)),
            );
        frame.render_widget(hint, area);
        return;
    }

    let all_titles: Vec<Line<'_>> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let label = workspace_label(&agent.workspace);
            let num = i + 1;
            let (badge, badge_color) = match (&agent.status, &agent.activity) {
                (AgentStatus::Exited(Some(0)), _) => (CHECK_MARK, theme.tab_badge_success),
                (AgentStatus::Exited(_), _) => (CROSS_MARK, theme.tab_badge_error),
                (AgentStatus::Running, AgentActivity::Idle) => (CIRCLE, theme.tab_badge_idle),
                (AgentStatus::Running, AgentActivity::Done) => {
                    (CHECK_MARK, theme.tab_badge_success)
                }
                (AgentStatus::Running, _) => {
                    let frame_idx = app.tick % SPINNER_FRAMES.len();
                    (SPINNER_FRAMES[frame_idx], theme.tab_badge_spinner)
                }
            };
            let mut spans = vec![
                Span::styled(format!(" {badge}"), Style::default().fg(badge_color)),
                Span::raw(format!(" {num}: ⌂ {label} ")),
            ];
            // Append unread dot for agents with unread completion events.
            if app.unread_agents.contains(&agent.id) {
                spans.push(Span::styled(CIRCLE, Style::default().fg(theme.tab_unread)));
                spans.push(Span::raw(" "));
            }
            Line::from(spans)
        })
        .collect();

    // Compute visible tab window that fits within the available width.
    // Each tab has its content width plus a 1-char divider between tabs.
    // Reserve some width for the block border (2 chars).
    let available = area.width.saturating_sub(2) as usize;
    let tab_widths: Vec<usize> = all_titles.iter().map(|t| t.width()).collect();

    let (titles, select_index) = visible_tab_window(&tab_widths, app.focused, available);

    // Compute click regions for each visible tab. The block has no left
    // border (Borders::BOTTOM only), so tabs start at area.x. Each tab
    // occupies its width in columns, separated by a 1-char divider.
    {
        let mut x = area.x;
        for entry in &titles {
            let w = match entry {
                VisibleTab::Tab(idx) => all_titles[*idx].width(),
                VisibleTab::Overflow(n) => format!(" +{n} more ").len(),
            };
            if let VisibleTab::Tab(idx) = entry {
                app.tab_rects
                    .push((Rect::new(x, area.y, w as u16, area.height), *idx));
            }
            // Advance past this tab + 1-char divider.
            x += w as u16 + 1;
        }
    }

    let visible_titles: Vec<Line<'_>> = titles
        .iter()
        .map(|entry| match entry {
            VisibleTab::Tab(idx) => all_titles[*idx].clone(),
            VisibleTab::Overflow(n) => Line::from(Span::styled(
                format!(" +{n} more "),
                Style::default()
                    .fg(theme.tab_overflow)
                    .add_modifier(Modifier::DIM),
            )),
        })
        .collect();

    let tabs = Tabs::new(visible_titles)
        .select(select_index)
        .highlight_style(
            Style::default()
                .fg(theme.tab_highlight)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
        .style(Style::default().fg(theme.tab_text))
        .divider(Span::raw("|"))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.tab_border))
                .title(
                    app.focused_agent()
                        .map(|a| {
                            format!(
                                " ◆ Agent {} │ ⌂ {} ",
                                app.focused + 1,
                                workspace_label(&a.workspace)
                            )
                        })
                        .unwrap_or_else(|| " Agents ".to_string()),
                )
                .title_style(
                    Style::default()
                        .fg(theme.tab_title)
                        .add_modifier(Modifier::BOLD),
                ),
        );

    frame.render_widget(tabs, area);
}

/// Produce a short label for a workspace path (basename, or full path if short).
fn workspace_label(path: &std::path::Path) -> String {
    let full = path.display().to_string();
    if full.len() <= 20 {
        return full;
    }
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or(full)
}

/// Entry in a visible tab window — either a real tab or an overflow indicator.
enum VisibleTab {
    /// Index into the full `all_titles` vec.
    Tab(usize),
    /// Number of hidden tabs represented by this overflow indicator.
    Overflow(usize),
}

/// Compute the window of tabs to display given the available width.
///
/// Returns a list of [`VisibleTab`] entries and the index within that list
/// corresponding to the focused tab. The focused tab is always included.
/// When all tabs don't fit, a `+N more` overflow indicator is appended or
/// prepended (or both).
fn visible_tab_window(
    tab_widths: &[usize],
    focused: usize,
    available: usize,
) -> (Vec<VisibleTab>, usize) {
    let n = tab_widths.len();
    if n == 0 {
        return (Vec::new(), 0);
    }

    // Clamp focused index so an out-of-bounds value doesn't panic.
    let focused = focused.min(n - 1);

    // The divider "|" between tabs takes 1 char.
    let total: usize = tab_widths.iter().sum::<usize>() + n.saturating_sub(1);
    if total <= available {
        // Everything fits — return all tabs as-is.
        let entries: Vec<VisibleTab> = (0..n).map(VisibleTab::Tab).collect();
        return (entries, focused);
    }

    // Compute overflow indicator width from the actual format string so it
    // stays correct if the format or tab count changes.
    let overflow_width = |hidden: usize| format!(" +{hidden} more ").len();

    // Greedily expand a window around the focused tab.
    let mut start = focused;
    let mut end = focused; // inclusive
    let mut used = tab_widths[focused];

    // Try to expand right first, then left, alternating.
    loop {
        let mut grew = false;

        // Try right.
        if end + 1 < n {
            // divider + tab width
            let cost = 1 + tab_widths[end + 1];
            let left_reserve = if start > 0 {
                overflow_width(start) + 1
            } else {
                0
            };
            let right_reserve = if end + 2 < n {
                overflow_width(n - end - 2) + 1
            } else {
                0
            };
            if used + cost + left_reserve + right_reserve <= available {
                end += 1;
                used += cost;
                grew = true;
            }
        }

        // Try left.
        if start > 0 {
            // divider + tab width
            let cost = 1 + tab_widths[start - 1];
            let left_reserve = if start > 1 {
                overflow_width(start - 1) + 1
            } else {
                0
            };
            let right_reserve = if end + 1 < n {
                overflow_width(n - end - 1) + 1
            } else {
                0
            };
            if used + cost + left_reserve + right_reserve <= available {
                start -= 1;
                used += cost;
                grew = true;
            }
        }

        if !grew {
            break;
        }
    }

    let mut entries = Vec::new();
    let mut select_index = 0;

    if start > 0 {
        entries.push(VisibleTab::Overflow(start));
    }

    for idx in start..=end {
        if idx == focused {
            select_index = entries.len();
        }
        entries.push(VisibleTab::Tab(idx));
    }

    if end + 1 < n {
        entries.push(VisibleTab::Overflow(n - end - 1));
    }

    (entries, select_index)
}

// ---------------------------------------------------------------------------
// Sidebar panel
// ---------------------------------------------------------------------------

/// Render the agent sidebar panel showing per-agent status information.
///
/// Each agent entry displays: status icon, agent number, workspace name,
/// current activity, and elapsed time. The sidebar-selected agent is
/// highlighted; the focused agent (whose output is in the main content
/// area) is marked with an arrow indicator.
fn render_sidebar(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // Clear previous sidebar rects.
    app.sidebar_rects.clear();

    if app.agents.is_empty() {
        let empty = Paragraph::new(" No agents")
            .style(Style::default().fg(theme.sidebar_dim))
            .block(
                Block::default()
                    .borders(Borders::RIGHT)
                    .border_style(Style::default().fg(theme.sidebar_border))
                    .title(" Agents ")
                    .title_style(
                        Style::default()
                            .fg(theme.sidebar_title)
                            .add_modifier(Modifier::BOLD),
                    ),
            );
        frame.render_widget(empty, area);
        return;
    }

    // Inner width available for text (area minus right border).
    let inner_width = area.width.saturating_sub(1) as usize;

    // Display agents in creation order (matching tab bar and 1-9 keybindings)
    // so the sidebar indices stay consistent with the rest of the UI.
    let indices: Vec<usize> = (0..app.agents.len()).collect();

    let mut lines: Vec<Line<'_>> = Vec::new();
    // Track (start_line, agent_index) for click regions.
    let mut entry_positions: Vec<(usize, usize)> = Vec::new();

    for &idx in &indices {
        let agent = &app.agents[idx];
        let is_focused = idx == app.focused;
        let is_selected = idx == app.sidebar_selected;

        let entry_start = lines.len();

        // Status icon.
        let (icon, icon_color) = match (&agent.status, &agent.activity) {
            (AgentStatus::Exited(Some(0)), _) => (CHECK_MARK, theme.sidebar_status_done),
            (AgentStatus::Exited(_), _) => (CROSS_MARK, theme.sidebar_status_error),
            (AgentStatus::Running, AgentActivity::Idle) => (CIRCLE, theme.sidebar_dim),
            (AgentStatus::Running, AgentActivity::Done) => (CHECK_MARK, theme.sidebar_status_done),
            (AgentStatus::Running, _) => {
                let frame_idx = app.tick % SPINNER_FRAMES.len();
                (SPINNER_FRAMES[frame_idx], theme.sidebar_status_running)
            }
        };

        // Focus indicator.
        let focus_indicator = if is_focused { ARROW_RIGHT } else { " " };

        // Agent name: number + workspace basename.
        let label = workspace_label(&agent.workspace);
        let num = idx + 1;

        // Activity label.
        let activity = match &agent.activity {
            AgentActivity::Idle => "idle".to_string(),
            AgentActivity::Thinking => "thinking".to_string(),
            AgentActivity::Tool(name) => {
                if name.len() > 10 {
                    format!("{}...", &name[..10])
                } else {
                    name.clone()
                }
            }
            AgentActivity::Done => "done".to_string(),
        };

        // Elapsed time.
        let elapsed = format_elapsed(agent.started_at.elapsed());

        // First line: focus indicator + icon + number: name.
        let name_text = format!("{num}: {label}");
        // Truncate name to fit.
        let max_name = inner_width.saturating_sub(5); // " > X N: "
        let display_name = if name_text.len() > max_name {
            format!("{}\u{2026}", &name_text[..max_name.saturating_sub(1)])
        } else {
            name_text
        };

        let row_style = if is_selected {
            Style::default()
                .bg(theme.sidebar_selected_bg)
                .fg(theme.sidebar_selected_fg)
        } else {
            Style::default().fg(theme.sidebar_fg)
        };

        lines.push(Line::from(vec![
            Span::styled(format!("{focus_indicator} "), row_style),
            Span::styled(format!("{icon} "), Style::default().fg(icon_color)),
            Span::styled(display_name, row_style.add_modifier(Modifier::BOLD)),
        ]));

        // Second line: activity + elapsed (indented).
        let detail = format!("    {activity} | {elapsed}");
        let detail_style = if is_selected {
            Style::default()
                .bg(theme.sidebar_selected_bg)
                .fg(theme.sidebar_dim)
        } else {
            Style::default().fg(theme.sidebar_dim)
        };
        lines.push(Line::from(Span::styled(detail, detail_style)));

        // Prompt snippet (third line, truncated).
        let prompt_snippet = agent.prompt.lines().next().unwrap_or("");
        let max_snippet = inner_width.saturating_sub(5);
        let snippet = if prompt_snippet.len() > max_snippet {
            format!(
                "    \"{}\u{2026}\"",
                &prompt_snippet[..max_snippet.saturating_sub(2)]
            )
        } else if !prompt_snippet.is_empty() {
            format!("    \"{prompt_snippet}\"")
        } else {
            String::new()
        };
        if !snippet.is_empty() {
            let snippet_style = if is_selected {
                Style::default()
                    .bg(theme.sidebar_selected_bg)
                    .fg(theme.sidebar_dim)
                    .add_modifier(Modifier::ITALIC)
            } else {
                Style::default()
                    .fg(theme.sidebar_dim)
                    .add_modifier(Modifier::ITALIC)
            };
            lines.push(Line::from(Span::styled(snippet, snippet_style)));
        }

        entry_positions.push((entry_start, idx));

        // Blank separator between agents.
        lines.push(Line::from(""));
    }

    // Remove trailing blank line.
    if lines.last().is_some_and(|l| l.width() == 0) {
        lines.pop();
    }

    // Compute sidebar click regions. The block has a top border (title row)
    // so inner content starts at area.y + 1. Each line is one row.
    {
        let inner_y = area.y + 1; // below block title
        let inner_w = area.width.saturating_sub(1); // minus right border
        let total_lines = lines.len();
        for (i, &(start_line, agent_idx)) in entry_positions.iter().enumerate() {
            let end_line = if i + 1 < entry_positions.len() {
                // Next entry's start minus the blank separator.
                entry_positions[i + 1].0.saturating_sub(1)
            } else {
                total_lines
            };
            let row_start = inner_y + start_line as u16;
            let row_count = (end_line - start_line) as u16;
            if row_start + row_count <= area.y + area.height {
                app.sidebar_rects
                    .push((Rect::new(area.x, row_start, inner_w, row_count), agent_idx));
            }
        }
    }

    let title = format!(" Agents ({}) ", app.agents.len());
    let sidebar = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::RIGHT)
                .border_style(Style::default().fg(theme.sidebar_border))
                .title(title)
                .title_style(
                    Style::default()
                        .fg(theme.sidebar_title)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.sidebar_bg).fg(theme.sidebar_fg));

    frame.render_widget(sidebar, area);
}

// ---------------------------------------------------------------------------
// Content area
// ---------------------------------------------------------------------------

/// Render the main content area with the focused agent's output.
fn render_content(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::NONE);

    let inner = block.inner(area);
    let theme = &app.theme;

    match app.focused_agent() {
        None => {
            render_welcome(theme, frame, area);
        }
        Some(_) => {
            let height = inner.height as usize;
            let focused = app.focused;
            let agent = &app.agents[focused];

            if agent.output.is_empty() {
                let spinner_frame = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
                let msg = Paragraph::new(Line::from(vec![
                    Span::styled(spinner_frame, Style::default().fg(theme.tab_badge_spinner)),
                    Span::styled(
                        " Agent is starting... waiting for output",
                        Style::default().fg(theme.content_empty),
                    ),
                ]))
                .alignment(Alignment::Center)
                .block(block);
                frame.render_widget(msg, area);
                return;
            }

            ensure_agent_cache(app, focused, inner.width);

            // Recompute search matches if the output has changed since last computation.
            // Done before taking an immutable borrow on the agent.
            {
                let gen = app.agents[focused].output_generation;
                if !app.search_query.text().is_empty() && gen != app.search_cache_generation {
                    app.recompute_search_matches();
                }
            }

            let agent = &app.agents[focused];
            let cached = agent.cached_lines.as_ref().unwrap();
            let total_rows = agent.cached_row_count;

            let max_scroll = total_rows.saturating_sub(height);

            // scroll_offset 0 = pinned to bottom (latest output).
            // Convert to scroll-from-top for Paragraph::scroll().
            let clamped_offset = agent.scroll_offset.min(max_scroll);
            let scroll_y = max_scroll.saturating_sub(clamped_offset) as u16;

            // Apply search highlighting if there are active search matches.
            // Otherwise, borrow from the cache to avoid deep-cloning every
            // Span/String each frame.
            let display_lines =
                if !app.search_matches.is_empty() && !app.search_query.text().is_empty() {
                    apply_search_highlights(
                        cached,
                        &app.search_matches,
                        app.search_match_index,
                        &app.theme,
                    )
                } else {
                    borrow_cached_lines(cached)
                };

            let paragraph = Paragraph::new(display_lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((scroll_y, 0));

            frame.render_widget(paragraph, area);
        }
    }
}

/// Create borrowed [`Line`]s from cached `Line<'static>` values.
///
/// Each [`Span`] borrows its string content via `Cow::Borrowed` from the
/// original cached span, avoiding the deep `String` clone that a plain
/// `Vec::clone()` would perform. New `Vec` containers are still allocated
/// for the line and span lists, but the (typically large) string payloads
/// are shared.
fn borrow_cached_lines<'a>(cached: &'a [Line<'static>]) -> Vec<Line<'a>> {
    cached
        .iter()
        .map(|line| {
            Line::from(
                line.spans
                    .iter()
                    .map(|span| Span::styled(span.content.as_ref(), span.style))
                    .collect::<Vec<_>>(),
            )
        })
        .collect()
}

/// Ensure the agent's cached lines are up to date. Rebuilds if stale.
fn ensure_agent_cache(app: &mut App, agent_idx: usize, inner_width: u16) {
    let agent = &app.agents[agent_idx];
    let cache_hit = agent.cached_lines.is_some()
        && agent.cache_generation == agent.output_generation
        && agent.cache_result_display == Some(app.result_display)
        && agent.cache_section_overrides_generation == agent.section_overrides_generation;

    if !cache_hit {
        let all_lines: Vec<&DisplayLine> = agent.output.iter().collect();
        let collapsed = collapse_tool_results(
            &all_lines,
            app.result_display,
            &agent.section_overrides,
            &app.theme,
        );
        let row_count = wrapped_row_count(&collapsed, inner_width);
        let owned = lines_to_static(collapsed);

        let agent_mut = &mut app.agents[agent_idx];
        agent_mut.cached_lines = Some(owned);
        agent_mut.cached_row_count = row_count;
        agent_mut.cache_generation = agent_mut.output_generation;
        agent_mut.cache_result_display = Some(app.result_display);
        agent_mut.cache_section_overrides_generation = agent_mut.section_overrides_generation;
    }
}

/// Render a single agent's output into a split pane with a bordered frame.
///
/// `agent_idx` is the Vec index of the agent to display. `is_focused` controls
/// whether the border is highlighted (bright) to indicate pane focus.
fn render_content_pane(
    app: &mut App,
    frame: &mut Frame,
    area: Rect,
    agent_idx: usize,
    is_focused: bool,
) {
    let theme = &app.theme;

    let agent = match app.agents.get(agent_idx) {
        Some(a) => a,
        None => return,
    };

    let label = workspace_label(&agent.workspace);
    let agent_num = agent_idx + 1;
    let title = format!(" Agent {agent_num}: {label} ");

    let border_color = if is_focused {
        theme.tab_highlight
    } else {
        theme.tab_border
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(
            Style::default()
                .fg(border_color)
                .add_modifier(if is_focused {
                    Modifier::BOLD
                } else {
                    Modifier::empty()
                }),
        )
        .title(title)
        .title_style(
            Style::default()
                .fg(if is_focused {
                    theme.tab_highlight
                } else {
                    theme.tab_text
                })
                .add_modifier(Modifier::BOLD),
        );

    let inner = block.inner(area);

    if agent.output.is_empty() {
        let spinner_frame = SPINNER_FRAMES[app.tick % SPINNER_FRAMES.len()];
        let msg = Paragraph::new(Line::from(vec![
            Span::styled(spinner_frame, Style::default().fg(theme.tab_badge_spinner)),
            Span::styled(
                " Waiting for output...",
                Style::default().fg(theme.content_empty),
            ),
        ]))
        .alignment(Alignment::Center)
        .block(block);
        frame.render_widget(msg, area);
        return;
    }

    let height = inner.height as usize;

    ensure_agent_cache(app, agent_idx, inner.width);

    let agent = &app.agents[agent_idx];
    let cached = agent.cached_lines.as_ref().unwrap();
    let total_rows = agent.cached_row_count;

    let max_scroll = total_rows.saturating_sub(height);
    let clamped_offset = agent.scroll_offset.min(max_scroll);
    let scroll_y = max_scroll.saturating_sub(clamped_offset) as u16;

    let display_lines = borrow_cached_lines(cached);

    let paragraph = Paragraph::new(display_lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y, 0));

    frame.render_widget(paragraph, area);
}

/// Render a centered welcome screen when no agents exist.
///
/// Displays the tool name, a brief description, and the most important
/// keybindings to help new users get started quickly.
fn render_welcome(theme: &Theme, frame: &mut Frame, area: Rect) {
    // Welcome content height: title(1) + blank(1) + border(1) + blank(1) +
    //   description(2) + blank(1) + keybindings header(1) + blank(1) +
    //   3 keybindings + blank(1) + border(1) + blank(1) + footer(1) = ~16 rows
    // Plus 2 for the outer block borders = 18 total.
    let popup_height: u16 = 18;
    let popup_width: u16 = 50;

    // Center the popup in the content area.
    let popup = centered_rect_fixed_height(
        // Convert fixed width to a percentage of the area, clamped to [30, 80].
        ((popup_width as u32 * 100 / area.width.max(1) as u32) as u16).clamp(30, 80),
        popup_height.min(area.height),
        area,
    );

    let title_style = Style::default()
        .fg(theme.welcome_title)
        .add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(theme.welcome_description);
    let key_style = Style::default()
        .fg(theme.welcome_key)
        .add_modifier(Modifier::BOLD);
    let key_desc_style = Style::default().fg(theme.welcome_key_desc);
    let border_line_style = Style::default().fg(theme.welcome_border);

    let mut text = vec![
        // Title block.
        Line::from(""),
        Line::from(Span::styled("Vorpal Agent Manager", title_style)).alignment(Alignment::Center),
        Line::from(""),
        Line::from(Span::styled(
            "────────────────────────────",
            border_line_style,
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        // Description.
        Line::from(Span::styled(
            "Launch and manage multiple Claude Code",
            desc_style,
        ))
        .alignment(Alignment::Center),
        Line::from(Span::styled(
            "agents from a single terminal interface.",
            desc_style,
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        // Quick-start keybindings header.
        Line::from(Span::styled("Quick Start", title_style)).alignment(Alignment::Center),
        Line::from(""),
    ];

    let bindings: &[(&str, &str)] = &[
        ("  n  ", "Create a new agent"),
        ("  ?  ", "Show all keybindings"),
        ("  q  ", "Quit"),
    ];

    for (key, desc) in bindings {
        text.push(
            Line::from(vec![
                Span::styled(*key, key_style),
                Span::styled(*desc, key_desc_style),
            ])
            .alignment(Alignment::Center),
        );
    }

    text.push(Line::from(""));
    text.push(
        Line::from(Span::styled(
            "Press 'n' to get started",
            Style::default()
                .fg(theme.welcome_title)
                .add_modifier(Modifier::DIM),
        ))
        .alignment(Alignment::Center),
    );

    let welcome = Paragraph::new(text).block(
        Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(theme.welcome_border))
            .title(" Welcome ")
            .title_style(title_style),
    );

    frame.render_widget(welcome, popup);
}

/// Compute the total number of terminal rows needed to display `lines` in a
/// viewport of the given `width`, accounting for word-wrapping.
///
/// Each [`Line`] takes at least one row; lines wider than `width` wrap to
/// additional rows (ceiling division).
fn wrapped_row_count(lines: &[Line<'_>], width: u16) -> usize {
    if width == 0 {
        return lines.len();
    }
    let w = width as usize;
    lines
        .iter()
        .map(|line| {
            let line_width = line.width();
            if line_width <= w {
                1
            } else {
                line_width.div_ceil(w)
            }
        })
        .sum()
}

/// Maximum length of tool result content in compact mode.
const COMPACT_RESULT_MAX: usize = 200;

/// Maximum number of first lines shown per tool result run in compact mode.
const COMPACT_RESULT_RUN_MAX: usize = 3;

/// Collapse consecutive runs of [`DisplayLine::ToolResult`] based on display mode.
///
/// - **Full**: all lines shown with full content.
/// - **Compact**: only the first [`COMPACT_RESULT_RUN_MAX`] lines of each run
///   are shown; individual lines are truncated to [`COMPACT_RESULT_MAX`] chars.
/// - **Hidden**: entire result runs are replaced with a single byte-count summary.
///
/// In all modes, consecutive tool result runs are tracked so that only the
/// first line displays the `⎿` connector; continuation lines use matching
/// whitespace indentation (Claude Code style).
///
/// `section_overrides` maps sequential ToolUse section indices to per-section
/// display modes that override the global `mode`.
///
/// Internally this is a three-stage pipeline:
/// 1. [`strip_leading_empty_after_turn_start`] — remove visual noise
/// 2. [`collapse_results`] — group lines into [`CollapsedBlock`]s
/// 3. [`render_to_spans`] — convert blocks into styled ratatui [`Line`]s
fn collapse_tool_results<'a>(
    lines: &[&'a DisplayLine],
    mode: ResultDisplay,
    section_overrides: &std::collections::HashMap<usize, ResultDisplay>,
    theme: &Theme,
) -> Vec<Line<'a>> {
    let stripped = strip_leading_empty_after_turn_start(lines);
    let blocks = collapse_results(&stripped, mode, section_overrides);
    render_to_spans(&blocks, theme)
}

// ---------------------------------------------------------------------------
// Pipeline stage 2: collapse into intermediate representation
// ---------------------------------------------------------------------------

/// Intermediate representation produced by [`collapse_results`].
///
/// Each variant captures the data needed by [`render_to_spans`] without
/// embedding any display logic, making both stages independently testable.
#[derive(Debug, Clone)]
enum CollapsedBlock<'a> {
    /// A run of consecutive tool results, already collapsed per the display mode.
    ToolResultRun(ToolResultRun<'a>),
    /// A ToolUse header with its effective display mode.
    ToolUseHeader {
        dl: &'a DisplayLine,
        effective_mode: ResultDisplay,
    },
    /// A run of consecutive text lines joined into markdown.
    TextRun(String),
    /// Any other single [`DisplayLine`] (Thinking, System, etc.).
    Single(&'a DisplayLine),
}

/// Collapsed representation of a consecutive run of [`DisplayLine::ToolResult`]s.
#[derive(Debug, Clone, PartialEq)]
enum ToolResultRun<'a> {
    /// Mode was [`ResultDisplay::Hidden`]: just the byte-count summary.
    Hidden { total_bytes: usize },
    /// Mode was [`ResultDisplay::Compact`]: up to [`COMPACT_RESULT_RUN_MAX`]
    /// visible lines, plus a count of hidden lines (may be 0).
    Compact {
        visible: Vec<ToolResultLine<'a>>,
        hidden_count: usize,
    },
    /// Mode was [`ResultDisplay::Full`]: every line from every entry.
    Full { lines: Vec<ToolResultLine<'a>> },
}

/// A single text line extracted from a `ToolResult`'s content.
#[derive(Debug, Clone, PartialEq)]
struct ToolResultLine<'a> {
    text: &'a str,
    is_error: bool,
}

/// Consume a consecutive run of [`DisplayLine::ToolResult`]s starting at
/// `start` and produce a [`ToolResultRun`] collapsed per `mode`.
///
/// Returns the constructed run and the new index after the last consumed
/// `ToolResult` line. Both the ToolUse-preceded path and the orphan path
/// in [`collapse_results`] delegate to this helper.
fn consume_tool_result_run<'a>(
    lines: &[&'a DisplayLine],
    start: usize,
    mode: ResultDisplay,
) -> (ToolResultRun<'a>, usize) {
    let mut i = start;
    let mut total_bytes: usize = 0;
    let mut total_lines: usize = 0;
    while i < lines.len() {
        if let DisplayLine::ToolResult { content, .. } = lines[i] {
            total_bytes += content.len();
            total_lines += content.lines().count().max(1);
            i += 1;
        } else {
            break;
        }
    }

    let collect_result_lines = || {
        let mut out = Vec::new();
        for dl in &lines[start..i] {
            if let DisplayLine::ToolResult {
                content, is_error, ..
            } = dl
            {
                for text_line in content.lines() {
                    out.push(ToolResultLine {
                        text: text_line,
                        is_error: *is_error,
                    });
                }
            }
        }
        out
    };

    let run = match mode {
        ResultDisplay::Hidden => ToolResultRun::Hidden { total_bytes },
        ResultDisplay::Compact => {
            let all = collect_result_lines();
            let skip = all.len().saturating_sub(COMPACT_RESULT_RUN_MAX);
            let visible: Vec<_> = all.into_iter().skip(skip).collect();
            let hidden_count = total_lines.saturating_sub(visible.len());
            ToolResultRun::Compact {
                visible,
                hidden_count,
            }
        }
        ResultDisplay::Full => ToolResultRun::Full {
            lines: collect_result_lines(),
        },
    };

    (run, i)
}

/// Group consecutive [`DisplayLine`]s into [`CollapsedBlock`]s.
///
/// ToolResult runs are measured and collapsed according to their effective
/// display mode (per-section override if present, otherwise `global_mode`).
/// ToolUse headers are emitted as `ToolUseHeader` blocks with their
/// effective mode. Text runs are gathered and joined. All other line types
/// pass through as `Single`.
fn collapse_results<'a>(
    lines: &[&'a DisplayLine],
    global_mode: ResultDisplay,
    section_overrides: &std::collections::HashMap<usize, ResultDisplay>,
) -> Vec<CollapsedBlock<'a>> {
    let mut blocks = Vec::with_capacity(lines.len());
    let mut i = 0;
    // Sequential counter for ToolUse sections.
    let mut section_counter: usize = 0;

    while i < lines.len() {
        if matches!(lines[i], DisplayLine::ToolUse { .. }) {
            let current_section = section_counter;
            section_counter += 1;
            let effective_mode = section_overrides
                .get(&current_section)
                .copied()
                .unwrap_or(global_mode);
            blocks.push(CollapsedBlock::ToolUseHeader {
                dl: lines[i],
                effective_mode,
            });
            i += 1;

            // Now consume the following ToolResult run (if any).
            if i < lines.len() && matches!(lines[i], DisplayLine::ToolResult { .. }) {
                let (run, new_i) = consume_tool_result_run(lines, i, effective_mode);
                i = new_i;
                blocks.push(CollapsedBlock::ToolResultRun(run));
            }
        } else if matches!(lines[i], DisplayLine::ToolResult { .. }) {
            // Orphan ToolResult without a preceding ToolUse — use global mode.
            let (run, new_i) = consume_tool_result_run(lines, i, global_mode);
            i = new_i;
            blocks.push(CollapsedBlock::ToolResultRun(run));
        } else if matches!(lines[i], DisplayLine::Text(_)) {
            let run_start = i;
            while i < lines.len() && matches!(lines[i], DisplayLine::Text(_)) {
                i += 1;
            }
            let markdown: String = lines[run_start..i]
                .iter()
                .filter_map(|dl| match dl {
                    DisplayLine::Text(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            blocks.push(CollapsedBlock::TextRun(markdown));
        } else {
            blocks.push(CollapsedBlock::Single(lines[i]));
            i += 1;
        }
    }

    blocks
}

// ---------------------------------------------------------------------------
// Pipeline stage 3: render collapsed blocks to ratatui spans
// ---------------------------------------------------------------------------

/// Convert a sequence of [`CollapsedBlock`]s into styled ratatui [`Line`]s.
fn render_to_spans<'a>(blocks: &[CollapsedBlock<'a>], theme: &Theme) -> Vec<Line<'a>> {
    let mut out: Vec<Line<'a>> = Vec::new();

    for block in blocks {
        match block {
            CollapsedBlock::ToolResultRun(run) => {
                render_tool_result_run(run, &mut out, theme);
            }
            CollapsedBlock::ToolUseHeader {
                dl, effective_mode, ..
            } => {
                out.extend(display_line_to_lines_with_indicator(
                    dl,
                    *effective_mode,
                    theme,
                ));
            }
            CollapsedBlock::TextRun(markdown) => {
                render_text_run(markdown, &mut out, theme);
            }
            CollapsedBlock::Single(dl) => {
                out.extend(display_line_to_lines(dl, theme));
            }
        }
    }

    out
}

/// Render a collapsed tool result run into output lines.
fn render_tool_result_run<'a>(run: &ToolResultRun<'a>, out: &mut Vec<Line<'a>>, theme: &Theme) {
    match run {
        ToolResultRun::Hidden { total_bytes } => {
            let size = if *total_bytes >= 1024 {
                format!("{:.1} KB", *total_bytes as f64 / 1024.0)
            } else {
                format!("{total_bytes} bytes")
            };
            out.push(Line::from(Span::styled(
                format!("  {RESULT_CONNECTOR}  {size} (press 'r' to cycle view)"),
                Style::default().fg(theme.tool_result_hidden),
            )));
        }
        ToolResultRun::Compact {
            visible,
            hidden_count,
        } => {
            for (idx, trl) in visible.iter().enumerate() {
                out.extend(render_tool_result(
                    trl.text,
                    trl.is_error,
                    ResultDisplay::Compact,
                    idx == 0,
                    theme,
                ));
            }
            if *hidden_count > 0 {
                out.push(Line::from(Span::styled(
                    format!(
                        "  {RESULT_CONNECTOR}  ... {hidden_count} more lines hidden (press 'r' to cycle view)"
                    ),
                    Style::default().fg(theme.tool_result_hidden),
                )));
            }
        }
        ToolResultRun::Full { lines } => {
            for (idx, trl) in lines.iter().enumerate() {
                out.extend(render_tool_result(
                    trl.text,
                    trl.is_error,
                    ResultDisplay::Full,
                    idx == 0,
                    theme,
                ));
            }
        }
    }
}

/// Render a markdown text run into output lines with block marker prefixes.
fn render_text_run<'a>(markdown: &str, out: &mut Vec<Line<'a>>, theme: &Theme) {
    let preprocessed = preprocess_markdown_tables(markdown);
    let rendered = tui_markdown::from_str(&preprocessed);
    // Prefix only the very first non-empty line with a ⏺ marker.
    // All subsequent lines (including those after blank lines) use
    // continuation indentation. This matches Claude Code's own
    // rendering style where each assistant text block gets a single
    // leading marker.
    //
    // Horizontal rules (`---`) are detected and rendered as Unicode
    // box-drawing separators instead of literal `---` text.
    let mut need_marker = true;
    for line in rendered.lines {
        // Detect horizontal rules: a single span whose trimmed
        // content is exactly `---` (what tui_markdown emits for
        // thematic breaks).
        let is_hr = line.spans.len() == 1 && line.spans[0].content.trim() == "---";

        if is_hr {
            let prefix = if need_marker {
                need_marker = false;
                Span::styled(
                    format!("{BLOCK_MARKER} "),
                    Style::default().fg(theme.block_marker),
                )
            } else {
                Span::raw(CONTINUATION_INDENT)
            };
            out.push(Line::from(vec![
                prefix,
                Span::styled(
                    "──────────────────────────────",
                    Style::default().fg(theme.text_hr),
                ),
            ]));
        } else if line.width() == 0 {
            out.push(Line::from(""));
        } else if need_marker {
            need_marker = false;
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                format!("{BLOCK_MARKER} "),
                Style::default().fg(theme.block_marker),
            )];
            spans.extend(
                line.spans
                    .into_iter()
                    .map(|s| Span::styled(s.content.into_owned(), s.style)),
            );
            out.push(Line::from(spans));
        } else {
            let mut spans: Vec<Span<'static>> = vec![Span::raw(CONTINUATION_INDENT)];
            spans.extend(
                line.spans
                    .into_iter()
                    .map(|s| Span::styled(s.content.into_owned(), s.style)),
            );
            out.push(Line::from(spans));
        }
    }
}

/// Convert a vector of [`Line`]s with borrowed data into fully-owned
/// `Line<'static>` values suitable for caching.
fn lines_to_static(lines: Vec<Line<'_>>) -> Vec<Line<'static>> {
    lines
        .into_iter()
        .map(|line| {
            let spans: Vec<Span<'static>> = line
                .spans
                .into_iter()
                .map(|span| Span::styled(span.content.into_owned(), span.style))
                .collect();
            Line::from(spans)
        })
        .collect()
}

/// Render a single line of tool result content.
///
/// The caller is responsible for splitting multiline `DisplayLine::ToolResult`
/// content on `\n` and passing individual lines here.
///
/// When `first_in_run` is true the line is prefixed with `  ⎿  ` (the result
/// connector). Continuation lines use matching whitespace indentation instead.
fn render_tool_result(
    content: &str,
    is_error: bool,
    mode: ResultDisplay,
    first_in_run: bool,
    theme: &Theme,
) -> Vec<Line<'static>> {
    /// Indentation that matches the width of `  ⎿  `.
    const CONTINUATION_INDENT: &str = "     ";

    let compact = mode == ResultDisplay::Compact;
    let display_content = if compact && content.len() > COMPACT_RESULT_MAX {
        let boundary = content
            .char_indices()
            .take_while(|&(i, _)| i < COMPACT_RESULT_MAX)
            .last()
            .map_or(0, |(i, c)| i + c.len_utf8());
        format!("{}\u{2026}", &content[..boundary])
    } else {
        content.to_string()
    };

    let prefix = if first_in_run {
        Span::styled(
            format!("  {RESULT_CONNECTOR}  "),
            Style::default().fg(theme.tool_result_connector),
        )
    } else {
        Span::raw(CONTINUATION_INDENT)
    };

    let mut spans = vec![prefix];
    if is_error {
        spans.push(Span::styled(
            "[ERROR] ",
            Style::default()
                .fg(theme.tool_result_error)
                .add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        display_content,
        Style::default().fg(theme.tool_result_text),
    ));
    vec![Line::from(spans)]
}

/// Remove empty `Text("")` lines that immediately follow a `TurnStart`.
///
/// Claude's first text delta often begins with `"\n\n"`, producing empty text
/// lines. These render as blank marker lines between the turn separator and
/// the first real content, creating unwanted whitespace — especially visible
/// at the very start of a session.
fn strip_leading_empty_after_turn_start<'a>(lines: &[&'a DisplayLine]) -> Vec<&'a DisplayLine> {
    let mut out = Vec::with_capacity(lines.len());
    let mut after_turn_start = false;

    for dl in lines {
        match dl {
            DisplayLine::TurnStart => {
                after_turn_start = true;
                out.push(*dl);
            }
            DisplayLine::Text(s) if after_turn_start && s.is_empty() => {
                // Skip empty text lines right after a TurnStart.
            }
            _ => {
                after_turn_start = false;
                out.push(*dl);
            }
        }
    }

    out
}

/// Wrap markdown table blocks in fenced code blocks so `tui_markdown` renders
/// them as preformatted text instead of dropping/concatenating the cells.
///
/// Consecutive lines starting with `|` are treated as a table block. Each block
/// is wrapped with triple-backtick fences so the pipe-aligned rows survive
/// rendering intact.
fn preprocess_markdown_tables(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_table = false;

    for line in input.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('|') {
            if !in_table {
                out.push_str("```\n");
                in_table = true;
            }
            out.push_str(line);
            out.push('\n');
        } else {
            if in_table {
                out.push_str("```\n");
                in_table = false;
            }
            out.push_str(line);
            out.push('\n');
        }
    }

    if in_table {
        out.push_str("```\n");
    }

    // Remove trailing newline added by our loop if the input didn't end with one.
    if !input.ends_with('\n') && out.ends_with('\n') {
        out.pop();
    }

    out
}

/// Collapse indicator characters.
const CHEVRON_EXPANDED: &str = "\u{25BC}"; // ▼
const CHEVRON_COLLAPSED: &str = "\u{25B6}"; // ▶

/// Render a ToolUse header line with a collapse/expand indicator.
///
/// Shows a chevron (▼ for full/expanded, ▶ for compact/hidden/collapsed) after
/// the tool name to indicate the display state of the following result section.
fn display_line_to_lines_with_indicator<'a>(
    dl: &'a DisplayLine,
    effective_mode: ResultDisplay,
    theme: &Theme,
) -> Vec<Line<'a>> {
    if let DisplayLine::ToolUse {
        tool,
        input_preview,
    } = dl
    {
        let color = theme.tool_color(tool);
        let chevron = match effective_mode {
            ResultDisplay::Full => CHEVRON_EXPANDED,
            ResultDisplay::Compact | ResultDisplay::Hidden => CHEVRON_COLLAPSED,
        };
        let mut spans = vec![
            Span::styled(
                format!("{BLOCK_MARKER} "),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                tool.to_string(),
                Style::default().fg(color).add_modifier(Modifier::BOLD),
            ),
        ];
        if !input_preview.is_empty() {
            spans.push(Span::styled(
                format!("({input_preview})"),
                Style::default().fg(theme.tool_input_preview),
            ));
        }
        spans.push(Span::styled(
            format!(" {chevron}"),
            Style::default()
                .fg(theme.tool_result_hidden)
                .add_modifier(Modifier::DIM),
        ));
        vec![Line::from(spans)]
    } else {
        display_line_to_lines(dl, theme)
    }
}

/// Convert a [`DisplayLine`] to one or more styled ratatui [`Line`]s.
///
/// Handles all variants except [`DisplayLine::Text`] and
/// [`DisplayLine::ToolResult`], which are rendered inline (via
/// `tui_markdown`) and by [`render_tool_result`] respectively with
/// run-position awareness.
fn display_line_to_lines<'a>(dl: &'a DisplayLine, theme: &Theme) -> Vec<Line<'a>> {
    match dl {
        // Text is handled by render_text_line() for run tracking.
        DisplayLine::Text(_) => Vec::new(),

        DisplayLine::ToolUse {
            tool,
            input_preview,
        } => {
            let color = theme.tool_color(tool);
            let mut spans = vec![
                Span::styled(
                    format!("{BLOCK_MARKER} "),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    tool.to_string(),
                    Style::default().fg(color).add_modifier(Modifier::BOLD),
                ),
            ];
            if !input_preview.is_empty() {
                spans.push(Span::styled(
                    format!("({input_preview})"),
                    Style::default().fg(theme.tool_input_preview),
                ));
            }
            vec![Line::from(spans)]
        }

        // ToolResult is handled by render_tool_result() for run tracking.
        DisplayLine::ToolResult { .. } => Vec::new(),

        DisplayLine::Thinking(s) => vec![Line::from(vec![
            Span::styled(
                format!("{BLOCK_MARKER} "),
                Style::default().fg(theme.thinking_color),
            ),
            Span::styled(
                s.as_str(),
                Style::default()
                    .fg(theme.thinking_color)
                    .add_modifier(Modifier::DIM | Modifier::ITALIC),
            ),
        ])],

        DisplayLine::Result(s) => vec![Line::from(vec![
            Span::styled(
                format!("{SESSION_MARKER} "),
                Style::default()
                    .fg(theme.result_marker)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(s.as_str(), Style::default().fg(theme.result_text)),
        ])],

        DisplayLine::System(s) => vec![Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(theme.system_text)
                .add_modifier(Modifier::ITALIC),
        ))],

        DisplayLine::Stderr(s) => vec![Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(theme.stderr_text)
                .add_modifier(Modifier::DIM),
        ))],

        DisplayLine::Error(s) => vec![Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(theme.error_text)
                .add_modifier(Modifier::BOLD),
        ))],

        DisplayLine::TurnStart => vec![Line::from("")],
    }
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

/// Format a [`Duration`] as a compact elapsed time string.
///
/// - Under 1 minute: `0:42`
/// - Under 1 hour: `1m23s`
/// - Under 24 hours: `1h05m`
/// - 24 hours or more: `1d2h`
fn format_elapsed(d: std::time::Duration) -> String {
    let total_secs = d.as_secs();
    let days = total_secs / 86400;
    let hours = (total_secs % 86400) / 3600;
    let mins = (total_secs % 3600) / 60;
    let secs = total_secs % 60;
    if days > 0 {
        format!("{d}d{h}h", d = days, h = hours)
    } else if hours > 0 {
        format!("{h}h{m:02}m", h = hours, m = mins)
    } else if mins > 0 {
        format!("{m}m{s:02}s", m = mins, s = secs)
    } else {
        format!("0:{s:02}", s = secs)
    }
}

/// Render the two-line status bar at the bottom.
fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // Split the 2-line area into two 1-line rows.
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    // Line 1: transient status message (if active) or agent status info.
    let status_line = if let Some(msg) = app.status_message() {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                msg,
                Style::default()
                    .fg(theme.status_message)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        match app.focused_agent() {
            None => Line::from(Span::styled(
                " No agent",
                Style::default().fg(theme.status_no_agent),
            )),
            Some(agent) => {
                let (activity_label, activity_color) = match &agent.activity {
                    AgentActivity::Idle => ("Idle", theme.activity_idle),
                    AgentActivity::Thinking => ("Thinking", theme.activity_thinking),
                    AgentActivity::Tool(_) => {
                        // Handled specially below for the formatted span.
                        ("", theme.activity_tool)
                    }
                    AgentActivity::Done => ("Done", theme.activity_done),
                };

                let activity_span = if let AgentActivity::Tool(name) = &agent.activity {
                    Span::styled(
                        format!("{ARROW_RIGHT} {name}"),
                        Style::default()
                            .fg(theme.activity_tool)
                            .add_modifier(Modifier::BOLD),
                    )
                } else {
                    Span::styled(
                        activity_label,
                        Style::default()
                            .fg(activity_color)
                            .add_modifier(Modifier::BOLD),
                    )
                };

                let sep = Span::styled(" | ", Style::default().fg(theme.status_separator));

                let elapsed = agent.started_at.elapsed();
                let elapsed_str = format_elapsed(elapsed);

                let scroll_info = if agent.scroll_offset == 0 {
                    "bottom".to_string()
                } else {
                    format!("-{}", agent.scroll_offset)
                };

                let mut spans = vec![
                    Span::raw(" "),
                    activity_span,
                    sep.clone(),
                    Span::raw(format!("⏱ {elapsed_str}")),
                    sep.clone(),
                    Span::raw(format!("↻ Turns: {}", agent.turn_count)),
                    sep.clone(),
                    Span::raw(format!("⚒ Tools: {}", agent.tool_count)),
                    sep.clone(),
                    Span::raw(format!("⇕ Scroll: {scroll_info}")),
                ];

                if agent.has_new_output {
                    spans.push(sep);
                    spans.push(Span::styled(
                        "\u{2193} new output",
                        Style::default()
                            .fg(theme.new_output)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                Line::from(spans)
            }
        }
    };

    let status_paragraph = Paragraph::new(status_line).style(
        Style::default()
            .bg(theme.status_bar_bg)
            .fg(theme.status_bar_fg),
    );
    frame.render_widget(status_paragraph, rows[0]);

    // Line 2: keybinding hints — priority-ordered, truncated to fit width.
    let hints = build_hint_bar(area.width);

    let hints_paragraph =
        Paragraph::new(hints).style(Style::default().bg(theme.hint_bar_bg).fg(theme.hint_bar_fg));
    frame.render_widget(hints_paragraph, rows[1]);
}

/// Build the hint bar line, fitting as many priority-ordered hints as possible.
///
/// At [`FULL_HINTS_WIDTH`]+ columns all hints are shown. Below that, hints are
/// added in priority order and a trailing `?:more` is appended when any are
/// truncated.
fn build_hint_bar(width: u16) -> Line<'static> {
    // Priority-ordered hint list. Each entry: (key, description, trailing_gap).
    // The trailing gap is the two-space separator that follows each hint.
    const HINTS: &[(&str, &str)] = &[
        ("n", "new"),
        ("s", "respond"),
        (":", "command"),
        ("/", "search"),
        ("Tab", "switch"),
        ("x", "kill"),
        ("q", "close"),
        ("?", "help"),
        ("y", "copy"),
        ("r", "results"),
        ("b", "sidebar"),
        ("|", "split"),
        ("t", "theme"),
        ("^C", "quit"),
        ("j/k", "scroll"),
        ("^D/^U", "page"),
        ("gg", "top"),
        ("G", "bottom"),
    ];

    // At wide terminals, show every hint without truncation.
    if width >= FULL_HINTS_WIDTH {
        let mut spans = Vec::new();
        spans.push(Span::raw(" "));
        for (i, (key, desc)) in HINTS.iter().enumerate() {
            spans.push(Span::styled(
                key.to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            if i + 1 < HINTS.len() {
                spans.push(Span::raw(format!(":{desc}  ")));
            } else {
                spans.push(Span::raw(format!(":{desc}")));
            }
        }
        return Line::from(spans);
    }

    // Narrow terminal: fit as many hints as possible, append "?:more" if
    // any were truncated.
    let available = width.saturating_sub(1) as usize; // 1 for leading space
                                                      // The truncation indicator "?:more" is always preceded by a 2-char
                                                      // separator ("  ") inherited from the last shown hint's trailing gap.
                                                      // We reserve the indicator width only (6 chars); the separator is already
                                                      // accounted for in `with_sep` of the preceding hint.
    let more_hint_width = "?:more".len();

    let mut spans = Vec::new();
    spans.push(Span::raw(" "));
    let mut used: usize = 1; // leading space

    for (shown, (key, desc)) in HINTS.iter().enumerate() {
        // Width of this hint: key + ":" + desc.
        let hint_text_width = key.len() + 1 + desc.len();
        let is_last = shown + 1 == HINTS.len();
        // Only add the 2-char separator when this isn't the last hint.
        let with_sep = hint_text_width + if is_last { 0 } else { 2 };

        // Check if this hint fits. If there are remaining hints after this one,
        // we must also reserve space for the "?:more" truncation indicator.
        // The 2-char separator before "?:more" comes from this hint's own
        // trailing "  " (included in `with_sep`), so we only reserve for the
        // indicator text itself.
        let remaining_after = HINTS.len() - shown - 1;
        let reserve = if remaining_after > 0 {
            more_hint_width
        } else {
            0
        };

        if used + with_sep + reserve > available && remaining_after > 0 {
            // Doesn't fit — append truncation indicator and stop.
            // The previous hint's trailing "  " separator provides the gap.
            spans.push(Span::styled(
                "?".to_string(),
                Style::default().add_modifier(Modifier::BOLD),
            ));
            spans.push(Span::raw(":more"));
            break;
        }

        spans.push(Span::styled(
            key.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        if !is_last {
            spans.push(Span::raw(format!(":{desc}  ")));
        } else {
            spans.push(Span::raw(format!(":{desc}")));
        }
        used += with_sep;
    }

    Line::from(spans)
}

// ---------------------------------------------------------------------------
// Input overlay
// ---------------------------------------------------------------------------

/// Return the label style for a field — highlighted/bold when active, dimmed otherwise.
fn field_label_style(active: bool, theme: &Theme) -> Style {
    if active {
        Style::default()
            .fg(theme.field_label_active)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.field_label_inactive)
    }
}

/// Render a centered input overlay for entering a new agent prompt, workspace,
/// and Claude Code options.
///
/// Also updates [`App::input_field_rects`] with the click regions for each
/// field so mouse clicks can focus the correct input field.
fn render_input(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // Compute inner width from the percentage-based horizontal layout so we
    // can calculate how many rows the input text will wrap to. The inner
    // content width is the popup width minus 2 (left + right border).
    let popup_width = (area.width as u32 * 60 / 100) as usize;
    let inner_width = popup_width.saturating_sub(2);

    // Count wrapped rows for each input field.
    let prompt_rows = wrapped_input_rows(app.input.text(), inner_width);
    let workspace_rows = wrapped_input_rows(app.workspace.text(), inner_width);

    // Build input lines — only the active field shows a cursor.
    let prompt_lines = build_input_field_lines(
        app.input.text(),
        app.input.cursor_pos(),
        app.input_field == InputField::Prompt,
        theme,
    );
    let workspace_lines = build_input_field_lines(
        app.workspace.text(),
        app.workspace.cursor_pos(),
        app.input_field == InputField::Workspace,
        theme,
    );

    let mut text = Vec::new();
    text.push(Line::from(Span::styled(
        "Prompt:",
        field_label_style(app.input_field == InputField::Prompt, theme),
    )));
    text.extend(prompt_lines);
    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
        "Workspace:",
        field_label_style(app.input_field == InputField::Workspace, theme),
    )));
    text.extend(workspace_lines);

    // In advanced mode, also show the 6 Claude option fields.
    let option_rows = if app.quick_launch {
        0
    } else {
        text.push(Line::from(""));

        // Claude option fields — selector fields use build_selector, others show
        // "(unset)" placeholder when empty and inactive.
        let perm_lines = build_selector(
            PERMISSION_MODES,
            app.permission_mode.text(),
            app.input_field == InputField::PermissionMode,
            theme,
        );
        let model_lines = build_selector(
            MODELS,
            app.model.text(),
            app.input_field == InputField::Model,
            theme,
        );
        let effort_lines = build_selector(
            EFFORT_LEVELS,
            app.effort.text(),
            app.input_field == InputField::Effort,
            theme,
        );
        let budget_lines = build_optional_field_lines(
            app.max_budget.text(),
            app.max_budget.cursor_pos(),
            app.input_field == InputField::MaxBudgetUsd,
            theme,
        );
        let tools_lines = build_optional_field_lines(
            app.allowed_tools.text(),
            app.allowed_tools.cursor_pos(),
            app.input_field == InputField::AllowedTools,
            theme,
        );
        let dir_lines = build_optional_field_lines(
            app.add_dir.text(),
            app.add_dir.cursor_pos(),
            app.input_field == InputField::AddDir,
            theme,
        );

        // Claude options header.
        text.push(Line::from(Span::styled(
            "── Claude Options ──",
            Style::default()
                .fg(theme.options_header)
                .add_modifier(Modifier::DIM),
        )));

        text.push(Line::from(Span::styled(
            "Permission Mode:",
            field_label_style(app.input_field == InputField::PermissionMode, theme),
        )));
        text.extend(perm_lines);
        text.push(Line::from(Span::styled(
            "Model:",
            field_label_style(app.input_field == InputField::Model, theme),
        )));
        text.extend(model_lines);
        text.push(Line::from(Span::styled(
            "Effort:",
            field_label_style(app.input_field == InputField::Effort, theme),
        )));
        text.extend(effort_lines);
        text.push(Line::from(Span::styled(
            "Max Budget USD:",
            field_label_style(app.input_field == InputField::MaxBudgetUsd, theme),
        )));
        text.extend(budget_lines);
        text.push(Line::from(Span::styled(
            "Allowed Tools (comma-separated):",
            field_label_style(app.input_field == InputField::AllowedTools, theme),
        )));
        text.extend(tools_lines);
        text.push(Line::from(Span::styled(
            "Add Dir (comma-separated):",
            field_label_style(app.input_field == InputField::AddDir, theme),
        )));
        text.extend(dir_lines);

        1 + 6 * 2 // header + 6 fields x (label + value)
    };

    // Footer hint varies by mode and active field.
    let footer = if app.quick_launch {
        "Enter: launch  |  Tab: next  |  ^E: advanced  |  ^P: templates  |  Esc: cancel"
    } else {
        match app.input_field {
            InputField::Prompt => {
                "Enter: newline  |  ^S: submit  |  ^P: templates  |  ^T: save  |  ^E: quick  |  Esc: cancel"
            }
            InputField::PermissionMode | InputField::Model | InputField::Effort => {
                "◀/▶: select  |  ^S: submit  |  ^P: templates  |  ^T: save  |  ^E: quick  |  Esc: cancel"
            }
            _ => "^S: submit  |  ^P: templates  |  ^T: save  |  ^E: quick  |  Esc: cancel",
        }
    };

    text.push(Line::from(""));
    text.push(
        Line::from(Span::styled(footer, Style::default().fg(theme.help_footer)))
            .alignment(Alignment::Center),
    );

    // Compute popup height based on content.
    let content_rows = if app.quick_launch {
        // Prompt label(1) + prompt_rows + blank(1) + Workspace label(1)
        // + workspace_rows + blank(1) + footer(1)
        1 + prompt_rows + 1 + 1 + workspace_rows + 1 + 1
    } else {
        // Quick fields + blank + options header + options + blank + footer
        1 + prompt_rows + 1 + 1 + workspace_rows + 1 + option_rows + 1 + 1
    };
    let popup_height = (content_rows + 2).min(area.height as usize) as u16;

    let popup = centered_rect_fixed_height(60, popup_height, area);

    // Clear the area behind the popup.
    frame.render_widget(Clear, popup);

    let inner_height = popup.height.saturating_sub(2) as usize;

    // Scroll so the cursor in the active field stays visible.
    //
    // Compute the wrapped row the cursor occupies, accounting for explicit
    // newlines in the buffer.
    let active_buf = match app.input_field {
        InputField::Prompt => &app.input,
        InputField::Workspace => &app.workspace,
        InputField::PermissionMode => &app.permission_mode,
        InputField::Model => &app.model,
        InputField::Effort => &app.effort,
        InputField::MaxBudgetUsd => &app.max_budget,
        InputField::AllowedTools => &app.allowed_tools,
        InputField::AddDir => &app.add_dir,
    };
    let (active_buffer, active_cursor) = (active_buf.text(), active_buf.cursor_pos());
    let cursor_wrapped_row = if inner_width > 0 {
        let before = &active_buffer[..active_cursor];
        let segments: Vec<&str> = before.split('\n').collect();
        let mut row = 0;
        for (i, seg) in segments.iter().enumerate() {
            let chars = seg.chars().count();
            if i < segments.len() - 1 {
                row += chars.max(1).div_ceil(inner_width);
            } else {
                row += chars / inner_width;
            }
        }
        row
    } else {
        0
    };

    // Compute the absolute row of the cursor within the full text layout.
    // This is cumulative: each field occupies (label_row + value_rows).
    let cursor_absolute_row = if app.quick_launch {
        // Quick mode: Prompt label(1) + prompt_rows + blank(1)
        //             + Workspace label(1) + workspace_rows
        match app.input_field {
            InputField::Prompt => 1 + cursor_wrapped_row,
            InputField::Workspace => 1 + prompt_rows + 2 + cursor_wrapped_row,
            // Other fields are not reachable in quick mode, but handle gracefully.
            _ => 1 + cursor_wrapped_row,
        }
    } else {
        // Advanced mode: Prompt label(1) + prompt_rows + blank(1) +
        //                Workspace label(1) + workspace_rows + blank(1) +
        //                header(1) + ... option fields ...
        let after_workspace = 1 + prompt_rows + 1 + 1 + workspace_rows + 1 + 1;
        // Each option field is label(1) + value(1) = 2 rows.
        match app.input_field {
            InputField::Prompt => 1 + cursor_wrapped_row,
            InputField::Workspace => 1 + prompt_rows + 2 + cursor_wrapped_row,
            InputField::PermissionMode => after_workspace + 1 + cursor_wrapped_row,
            InputField::Model => after_workspace + 2 + 1 + cursor_wrapped_row,
            InputField::Effort => after_workspace + 4 + 1 + cursor_wrapped_row,
            InputField::MaxBudgetUsd => after_workspace + 6 + 1 + cursor_wrapped_row,
            InputField::AllowedTools => after_workspace + 8 + 1 + cursor_wrapped_row,
            InputField::AddDir => after_workspace + 10 + 1 + cursor_wrapped_row,
        }
    };
    let scroll_y = if cursor_absolute_row >= inner_height {
        (cursor_absolute_row - inner_height + 1) as u16
    } else {
        0
    };

    // Compute input field click regions for mouse support.
    // Each field spans its label row + value rows. The popup inner area
    // starts at (popup.x + 1, popup.y + 1) and is offset by scroll_y.
    {
        app.input_field_rects.clear();
        let inner_x = popup.x + 1;
        let inner_y = popup.y + 1;
        let inner_w = popup.width.saturating_sub(2);

        // Helper: convert a content row to a screen y, returning None if scrolled off.
        let screen_y = |content_row: usize| -> Option<u16> {
            let scrolled = content_row as i32 - scroll_y as i32;
            if scrolled >= 0 && scrolled < inner_height as i32 {
                Some(inner_y + scrolled as u16)
            } else {
                None
            }
        };

        // Prompt: label at row 0, value rows 1..1+prompt_rows.
        let prompt_start = 0_usize;
        let prompt_end = 1 + prompt_rows;
        // Workspace: label at prompt_end+1, value rows prompt_end+2..
        let ws_label = prompt_end + 1;
        let ws_end = ws_label + 1 + workspace_rows;

        let fields_quick: &[(usize, usize, InputField)] = &[
            (prompt_start, prompt_end, InputField::Prompt),
            (ws_label, ws_end, InputField::Workspace),
        ];

        let register =
            |start: usize, end: usize, field: InputField, rects: &mut Vec<(Rect, InputField)>| {
                if let (Some(sy), Some(ey)) = (screen_y(start), screen_y(end.saturating_sub(1))) {
                    let h = ey - sy + 1;
                    rects.push((Rect::new(inner_x, sy, inner_w, h), field));
                }
            };

        for &(start, end, field) in fields_quick {
            register(start, end, field, &mut app.input_field_rects);
        }

        if !app.quick_launch {
            // Advanced mode fields start after workspace + blank + header.
            let base = ws_end + 1 + 1; // blank + header
            let adv_fields: &[(usize, InputField)] = &[
                (0, InputField::PermissionMode),
                (2, InputField::Model),
                (4, InputField::Effort),
                (6, InputField::MaxBudgetUsd),
                (8, InputField::AllowedTools),
                (10, InputField::AddDir),
            ];
            for &(offset, field) in adv_fields {
                let start = base + offset;
                let end = start + 2;
                register(start, end, field, &mut app.input_field_rects);
            }
        }
    }

    // Title includes a mode indicator and context.
    let title = if let Some(target_id) = app.respond_target {
        let label = app
            .agent_vec_index(target_id)
            .map(|idx| format!("{}", idx + 1))
            .unwrap_or_else(|| {
                tracing::debug!(target_id, "respond target has no Vec index");
                "?".to_string()
            });
        format!(" Respond to Agent {} ", label)
    } else if app.quick_launch {
        " New Agent (quick) ".to_string()
    } else {
        " New Agent (advanced) ".to_string()
    };

    let input_widget = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.input_border))
                .title(title)
                .title_style(
                    Style::default()
                        .fg(theme.input_title)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.input_bg).fg(theme.input_fg));

    frame.render_widget(input_widget, popup);
}

/// Count how many terminal rows an input string occupies when wrapped to `width`.
///
/// Accounts for explicit newlines: each `\n`-delimited segment is wrapped
/// independently, and the results are summed.
fn wrapped_input_rows(text: &str, width: usize) -> usize {
    if width == 0 {
        return text.split('\n').count().max(1);
    }
    text.split('\n')
        .map(|line| {
            let chars = line.chars().count().max(1); // empty line still takes 1 row
            chars.div_ceil(width)
        })
        .sum::<usize>()
        .max(1)
}

/// Build a horizontal selector row from a list of options.
///
/// Displays all options inline with `│` separators. The currently selected
/// option is highlighted (cyan when active, white when inactive).
fn build_selector(
    options: &[&str],
    current: &str,
    active: bool,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    for (i, option) in options.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(
                " │ ",
                Style::default().fg(theme.selector_separator),
            ));
        }

        let is_selected = *option == current;

        if is_selected {
            spans.push(Span::styled(
                option.to_string(),
                Style::default()
                    .fg(if active {
                        theme.selector_active
                    } else {
                        theme.selector_inactive
                    })
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            ));
        } else {
            spans.push(Span::styled(
                option.to_string(),
                Style::default().fg(theme.selector_option_dim),
            ));
        }
    }

    vec![Line::from(spans)]
}

/// Build input field lines for an optional Claude option.
///
/// Shows a dim "(unset)" placeholder when the buffer is empty and the field is
/// not active. Otherwise delegates to [`build_input_field_lines`].
fn build_optional_field_lines(
    buffer: &str,
    cursor: usize,
    active: bool,
    theme: &Theme,
) -> Vec<Line<'static>> {
    if buffer.is_empty() && !active {
        vec![Line::from(Span::styled(
            "(unset)",
            Style::default()
                .fg(theme.option_unset)
                .add_modifier(Modifier::DIM),
        ))]
    } else {
        build_input_field_lines(buffer, cursor, active, theme)
    }
}

/// Build one or more input field lines with optional cursor highlight.
///
/// Splits the buffer on `\n` so that multiline content (e.g. the prompt
/// field) renders correctly. The cursor highlight block is placed on the
/// appropriate segment.
fn build_input_field_lines(
    buffer: &str,
    cursor: usize,
    active: bool,
    theme: &Theme,
) -> Vec<Line<'static>> {
    let segments: Vec<&str> = buffer.split('\n').collect();

    if !active {
        return segments
            .iter()
            .map(|s| Line::from(Span::raw(s.to_string())))
            .collect();
    }

    let mut result = Vec::with_capacity(segments.len());
    let mut byte_offset: usize = 0;
    let mut cursor_placed = false;

    for (i, segment) in segments.iter().enumerate() {
        let seg_start = byte_offset;
        let seg_end = seg_start + segment.len();

        // Place the cursor highlight on the first matching segment.
        let cursor_here = !cursor_placed && cursor >= seg_start && cursor <= seg_end;

        if cursor_here {
            cursor_placed = true;
            let local = cursor - seg_start;
            let before = &segment[..local];
            let after = &segment[local..];
            let clen = after.chars().next().map_or(0, |c| c.len_utf8());

            result.push(Line::from(vec![
                Span::raw(before.to_string()),
                Span::styled(
                    if after.is_empty() {
                        " ".to_string()
                    } else {
                        after[..clen].to_string()
                    },
                    Style::default().bg(theme.cursor_bg).fg(theme.cursor_fg),
                ),
                Span::raw(after.get(clen..).unwrap_or("").to_string()),
            ]));
        } else {
            result.push(Line::from(Span::raw(segment.to_string())));
        }

        // Advance past this segment + the '\n' separator.
        if i < segments.len() - 1 {
            byte_offset = seg_end + 1;
        }
    }

    result
}

// ---------------------------------------------------------------------------
// Template picker overlay
// ---------------------------------------------------------------------------

/// Render the template picker overlay shown when the user presses `n`.
///
/// Displays a list of available templates with "Blank" as the first option.
/// The user navigates with Up/Down and selects with Enter.
fn render_template_picker(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // Build the list of items: "Blank" + all templates.
    let total = 1 + app.template_list.len();
    let content_rows = total + 3; // items + blank + footer + header-gap
    let popup_height = (content_rows + 2).min(area.height as usize) as u16;

    let popup = centered_rect_fixed_height(50, popup_height, area);
    frame.render_widget(Clear, popup);

    let mut text = Vec::new();

    // "Blank" option.
    let blank_style = if app.template_selected == 0 {
        Style::default()
            .fg(theme.input_fg)
            .bg(theme.selector_active)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(theme.input_fg)
    };
    text.push(Line::from(Span::styled(
        "  (blank) -- start from scratch",
        blank_style,
    )));

    // Template entries.
    for (i, tmpl) in app.template_list.iter().enumerate() {
        let is_selected = app.template_selected == i + 1;
        let label = if tmpl.builtin {
            format!(
                "  {} [built-in]{}",
                tmpl.name,
                tmpl.permission_mode
                    .as_deref()
                    .map(|pm| format!("  ({})", pm))
                    .unwrap_or_default()
            )
        } else {
            format!(
                "  {}{}",
                tmpl.name,
                tmpl.permission_mode
                    .as_deref()
                    .map(|pm| format!("  ({})", pm))
                    .unwrap_or_default()
            )
        };

        let style = if is_selected {
            Style::default()
                .fg(theme.input_fg)
                .bg(theme.selector_active)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(theme.input_fg)
        };

        text.push(Line::from(Span::styled(label, style)));
    }

    // Footer.
    text.push(Line::from(""));
    text.push(
        Line::from(Span::styled(
            "Up/Down: navigate  |  Enter: select  |  d: delete  |  Esc: cancel",
            Style::default().fg(theme.help_footer),
        ))
        .alignment(Alignment::Center),
    );

    let picker = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.input_border))
                .title(" Select Template ")
                .title_style(
                    Style::default()
                        .fg(theme.input_title)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.input_bg).fg(theme.input_fg));

    frame.render_widget(picker, popup);
}

// ---------------------------------------------------------------------------
// Save-template dialog overlay
// ---------------------------------------------------------------------------

/// Render the save-template name dialog on top of the input form.
fn render_save_template_dialog(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let popup = centered_rect_fixed_height(50, 7, area);
    frame.render_widget(Clear, popup);

    let name_text = app.template_name_input.text();
    let cursor = app.template_name_input.cursor_pos();

    let name_line = if name_text.is_empty() {
        vec![Span::styled(
            " ",
            Style::default().bg(theme.cursor_bg).fg(theme.cursor_fg),
        )]
    } else {
        let before = &name_text[..cursor];
        let after = &name_text[cursor..];
        let clen = after.chars().next().map_or(0, |c| c.len_utf8());
        vec![
            Span::raw(before.to_string()),
            Span::styled(
                if after.is_empty() {
                    " ".to_string()
                } else {
                    after[..clen].to_string()
                },
                Style::default().bg(theme.cursor_bg).fg(theme.cursor_fg),
            ),
            Span::raw(after.get(clen..).unwrap_or("").to_string()),
        ]
    };

    let text = vec![
        Line::from(Span::styled(
            "Template name:",
            Style::default()
                .fg(theme.field_label_active)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(name_line),
        Line::from(""),
        Line::from(Span::styled(
            "Enter: save  |  Esc: cancel",
            Style::default().fg(theme.help_footer),
        ))
        .alignment(Alignment::Center),
    ];

    let dialog = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.input_border))
                .title(" Save Template ")
                .title_style(
                    Style::default()
                        .fg(theme.input_title)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.input_bg).fg(theme.input_fg));

    frame.render_widget(dialog, popup);
}

// ---------------------------------------------------------------------------
// Confirm-close overlay
// ---------------------------------------------------------------------------

/// Render a confirmation dialog when the user tries to close a running agent.
fn render_confirm_close(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let popup = centered_rect(50, 30, area);
    frame.render_widget(Clear, popup);

    let agent_num = app.focused + 1;
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Agent {agent_num} is still running."),
            Style::default()
                .fg(theme.confirm_text)
                .add_modifier(Modifier::BOLD),
        ))
        .alignment(Alignment::Center),
        Line::from(""),
        Line::from("Closing will stop the Claude").alignment(Alignment::Center),
        Line::from("instance for this tab.").alignment(Alignment::Center),
        Line::from(""),
        Line::from(vec![
            Span::raw("Close anyway? "),
            Span::styled(
                "y",
                Style::default()
                    .fg(theme.confirm_yes)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("/"),
            Span::styled(
                "n",
                Style::default()
                    .fg(theme.confirm_no)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center),
        Line::from(""),
    ];

    let confirm = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.confirm_border))
                .title(" Confirm Close ")
                .title_style(
                    Style::default()
                        .fg(theme.confirm_title)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.confirm_bg).fg(theme.confirm_fg));

    frame.render_widget(confirm, popup);
}

// ---------------------------------------------------------------------------
// Help overlay
// ---------------------------------------------------------------------------

/// Render a centered help overlay showing all keybindings.
fn render_help(theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 70, area);

    // Clear the area behind the popup.
    frame.render_widget(Clear, popup);

    // Key-description pairs for the help table.
    const KEY_COL_WIDTH: usize = 16;
    const GAP: usize = 2;
    let bindings: Vec<(&str, &str)> = vec![
        ("Tab / l", "Next agent"),
        ("Shift+Tab / h", "Previous agent"),
        ("1-9", "Focus agent by number"),
        ("n", "New agent (quick launch)"),
        ("s", "Respond to exited agent (session)"),
        ("x", "Kill focused agent"),
        ("y", "Copy output to clipboard"),
        (":", "Open command palette"),
        ("/", "Search output"),
        ("n / N", "Next / previous match (in search)"),
        ("j / Down", "Scroll down (toward latest)"),
        ("k / Up", "Scroll up (into history)"),
        ("Ctrl+D / PgDn", "Half-page down"),
        ("Ctrl+U / PgUp", "Half-page up"),
        ("Ctrl+F", "Full-page down"),
        ("Ctrl+B", "Full-page up"),
        ("gg", "Scroll to top"),
        ("G", "Jump to bottom (latest)"),
        ("r", "Toggle compact results"),
        ("b", "Toggle sidebar panel"),
        ("J / K", "Navigate sidebar selection"),
        ("|", "Toggle split-pane view"),
        ("`", "Switch split-pane focus"),
        ("t", "Cycle color theme"),
        ("q", "Close focused tab"),
        ("Ctrl+C", "Quit all agents"),
        ("?", "Toggle this help"),
    ];

    // Compute the maximum line width so every line can be padded equally.
    // Each line is: KEY_COL_WIDTH + GAP + desc.len()
    let max_desc_len = bindings.iter().map(|(_, d)| d.len()).max().unwrap_or(0);
    let max_line_width = KEY_COL_WIDTH + GAP + max_desc_len;

    // Build keybinding lines, each padded to the same total width.
    let mut help_text: Vec<Line<'_>> = Vec::new();

    let heading_style = Style::default()
        .fg(theme.help_heading)
        .add_modifier(Modifier::BOLD);
    let footer_style = Style::default().fg(theme.help_footer);

    help_text.push(help_line_padded(
        "Keybindings",
        max_line_width,
        heading_style,
    ));
    help_text.push(Line::from(" ".repeat(max_line_width)));

    for (key, desc) in &bindings {
        let key_span = Span::styled(
            format!("{key:>width$}", width = KEY_COL_WIDTH),
            Style::default()
                .fg(theme.help_key)
                .add_modifier(Modifier::BOLD),
        );
        let used = KEY_COL_WIDTH + GAP + desc.len();
        let trailing = max_line_width.saturating_sub(used);
        let mut spans = vec![key_span, Span::raw("  "), Span::raw(*desc)];
        if trailing > 0 {
            spans.push(Span::raw(" ".repeat(trailing)));
        }
        help_text.push(Line::from(spans));
    }

    help_text.push(Line::from(" ".repeat(max_line_width)));
    help_text.push(help_line_padded(
        "Commands (: to open)",
        max_line_width,
        heading_style,
    ));
    help_text.push(Line::from(" ".repeat(max_line_width)));

    for cmd in COMMANDS {
        let key_span = Span::styled(
            format!(
                "{:>width$}",
                format!(":{}", cmd.name),
                width = KEY_COL_WIDTH
            ),
            Style::default()
                .fg(theme.help_key)
                .add_modifier(Modifier::BOLD),
        );
        let used = KEY_COL_WIDTH + GAP + cmd.description.len();
        let trailing = max_line_width.saturating_sub(used);
        let mut spans = vec![key_span, Span::raw("  "), Span::raw(cmd.description)];
        if trailing > 0 {
            spans.push(Span::raw(" ".repeat(trailing)));
        }
        help_text.push(Line::from(spans));
    }

    help_text.push(Line::from(" ".repeat(max_line_width)));
    help_text.push(help_line_padded(
        &format!("Min terminal size: {}x{}", MIN_WIDTH, MIN_HEIGHT),
        max_line_width,
        heading_style,
    ));
    help_text.push(Line::from(" ".repeat(max_line_width)));
    help_text.push(help_line_padded(
        "Press ? to close",
        max_line_width,
        footer_style,
    ));

    let help = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.help_border))
                .title(" Help ")
                .title_style(
                    Style::default()
                        .fg(theme.help_title)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.help_bg).fg(theme.help_fg));

    frame.render_widget(help, popup);
}

/// Build a centered label line padded to `width` so it aligns with the
/// keybinding block when `Alignment::Center` is applied.
fn help_line_padded(text: &str, width: usize, style: Style) -> Line<'static> {
    let text_len = text.len();
    let total_pad = width.saturating_sub(text_len);
    let left_pad = total_pad / 2;
    let right_pad = total_pad - left_pad;
    Line::from(vec![
        Span::raw(" ".repeat(left_pad)),
        Span::styled(text.to_string(), style),
        Span::raw(" ".repeat(right_pad)),
    ])
}

/// Compute a centered rectangle that takes up `percent_x`% width and
/// `percent_y`% height of `area`.
fn centered_rect(percent_x: u16, percent_y: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Percentage((100 - percent_y) / 2),
        Constraint::Percentage(percent_y),
        Constraint::Percentage((100 - percent_y) / 2),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

/// Compute a centered rectangle with `percent_x`% width and a fixed `height`
/// in rows, vertically centered in `area`.
fn centered_rect_fixed_height(percent_x: u16, height: u16, area: Rect) -> Rect {
    let vertical = Layout::vertical([
        Constraint::Fill(1),
        Constraint::Length(height),
        Constraint::Fill(1),
    ])
    .split(area);

    Layout::horizontal([
        Constraint::Percentage((100 - percent_x) / 2),
        Constraint::Percentage(percent_x),
        Constraint::Percentage((100 - percent_x) / 2),
    ])
    .split(vertical[1])[1]
}

// ---------------------------------------------------------------------------
// Search bar and highlighting
// ---------------------------------------------------------------------------

/// Render the search bar overlay on top of the status area (replaces the
/// hint bar row).
fn render_search_bar(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // The status area is 2 rows. Render the search bar on the second row
    // (the hint bar position).
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    let query = app.search_query.text();
    let match_info = if query.is_empty() {
        String::new()
    } else if app.search_matches.is_empty() {
        " [no matches]".to_string()
    } else {
        format!(
            " [{} of {}]",
            app.search_match_index + 1,
            app.search_matches.len()
        )
    };

    let cursor_pos = app.search_query.cursor_pos();
    let before = &query[..cursor_pos];
    let after = &query[cursor_pos..];
    let cursor_char_len = after.chars().next().map_or(0, |c| c.len_utf8());

    let mut spans = vec![
        Span::styled(
            "/",
            Style::default()
                .fg(theme.search_bar_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(before.to_string()),
    ];

    // Cursor block.
    spans.push(Span::styled(
        if after.is_empty() {
            " ".to_string()
        } else {
            after[..cursor_char_len].to_string()
        },
        Style::default().bg(theme.cursor_bg).fg(theme.cursor_fg),
    ));
    if cursor_char_len < after.len() {
        spans.push(Span::raw(after[cursor_char_len..].to_string()));
    }

    spans.push(Span::styled(
        match_info,
        Style::default().fg(theme.search_bar_fg),
    ));

    let search_line = Line::from(spans);
    let search_paragraph = Paragraph::new(search_line)
        .style(Style::default().bg(theme.hint_bar_bg).fg(theme.hint_bar_fg));
    frame.render_widget(search_paragraph, rows[1]);
}

// ---------------------------------------------------------------------------
// Command palette overlay
// ---------------------------------------------------------------------------

/// Render the command palette: an input bar at the bottom with a filtered
/// dropdown of matching commands above it.
fn render_command_palette(app: &App, frame: &mut Frame, status_area: Rect) {
    let theme = &app.theme;

    // The status area is 2 rows. Render the command bar on the second row
    // (the hint bar position), same as the search bar.
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(status_area);

    let query = app.command_input.text();
    let filtered = &app.command_filtered;

    // -- Dropdown above the status bar ----------------------------------------
    if !filtered.is_empty() {
        let dropdown_height = filtered.len().min(8) as u16;
        let dropdown_y = status_area.y.saturating_sub(dropdown_height);
        let dropdown_area = Rect::new(0, dropdown_y, status_area.width, dropdown_height);

        frame.render_widget(Clear, dropdown_area);

        let mut dropdown_lines: Vec<Line<'static>> = Vec::new();
        for (i, cmd) in filtered.iter().enumerate() {
            let is_selected = i == app.command_selected;
            let (name_style, desc_style) = if is_selected {
                (
                    Style::default()
                        .fg(theme.command_selected_fg)
                        .bg(theme.command_selected_bg)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.command_desc_fg)
                        .bg(theme.command_selected_bg),
                )
            } else {
                (
                    Style::default()
                        .fg(theme.command_match_fg)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(theme.command_desc_fg),
                )
            };

            dropdown_lines.push(Line::from(vec![
                Span::styled(if is_selected { " > " } else { "   " }, name_style),
                Span::styled(format!(":{}", cmd.name), name_style),
                Span::styled(format!("  {}", cmd.description), desc_style),
            ]));
        }

        let dropdown = Paragraph::new(dropdown_lines).style(Style::default().bg(theme.hint_bar_bg));
        frame.render_widget(dropdown, dropdown_area);
    }

    // -- Command input bar (bottom row) ---------------------------------------
    let cursor_pos = app.command_input.cursor_pos();
    let before = &query[..cursor_pos];
    let after = &query[cursor_pos..];
    let cursor_char_len = after.chars().next().map_or(0, |c| c.len_utf8());

    let mut spans = vec![
        Span::styled(
            ":",
            Style::default()
                .fg(theme.command_bar_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(before.to_string()),
    ];

    // Cursor block.
    spans.push(Span::styled(
        if after.is_empty() {
            " ".to_string()
        } else {
            after[..cursor_char_len].to_string()
        },
        Style::default().bg(theme.cursor_bg).fg(theme.cursor_fg),
    ));
    if cursor_char_len < after.len() {
        spans.push(Span::raw(after[cursor_char_len..].to_string()));
    }

    // Show error message if present.
    if let Some(err) = &app.command_error {
        spans.push(Span::styled(
            format!("  {err}"),
            Style::default().fg(theme.command_error_fg),
        ));
    }

    let command_line = Line::from(spans);
    let command_paragraph = Paragraph::new(command_line)
        .style(Style::default().bg(theme.hint_bar_bg).fg(theme.hint_bar_fg));
    frame.render_widget(command_paragraph, rows[1]);
}

// ---------------------------------------------------------------------------
// History search overlay
// ---------------------------------------------------------------------------

/// Maximum number of history entries shown in the Ctrl+R search dropdown.
const MAX_VISIBLE_HISTORY: usize = 10;

/// Render the Ctrl+R history search overlay on top of the input form.
///
/// Shows a search bar and a filtered list of previous prompts. The currently
/// selected entry is highlighted.
fn render_history_search(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    let visible_count = app.history_search_results.len().min(MAX_VISIBLE_HISTORY);
    // Height: 1 search bar + visible entries + 2 border rows.
    let popup_height = (visible_count as u16 + 3).min(area.height);

    let popup = centered_rect_fixed_height(60, popup_height, area);

    frame.render_widget(Clear, popup);

    if popup.height < 3 || popup.width < 10 {
        return;
    }

    let inner_area = Rect::new(
        popup.x + 1,
        popup.y + 1,
        popup.width.saturating_sub(2),
        popup.height.saturating_sub(2),
    );

    // Render the border block.
    let border_block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.input_border))
        .title(" History Search (Ctrl+R) ")
        .title_style(
            Style::default()
                .fg(theme.input_title)
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().bg(theme.input_bg).fg(theme.input_fg));
    frame.render_widget(border_block, popup);

    if inner_area.height == 0 || inner_area.width == 0 {
        return;
    }

    // Split inner area: 1 row for search bar, rest for results.
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Fill(1)]).split(inner_area);

    // Render search input bar.
    let query = app.history_search_query.text();
    let cursor_pos = app.history_search_query.cursor_pos();
    let before = &query[..cursor_pos];
    let after = &query[cursor_pos..];
    let cursor_char_len = after.chars().next().map_or(0, |c| c.len_utf8());

    let mut search_spans = vec![
        Span::styled(
            "search: ",
            Style::default()
                .fg(theme.search_bar_fg)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw(before.to_string()),
    ];
    search_spans.push(Span::styled(
        if after.is_empty() {
            " ".to_string()
        } else {
            after[..cursor_char_len].to_string()
        },
        Style::default().bg(theme.cursor_bg).fg(theme.cursor_fg),
    ));
    if cursor_char_len < after.len() {
        search_spans.push(Span::raw(after[cursor_char_len..].to_string()));
    }

    let match_info = if app.history_search_results.is_empty() && !query.is_empty() {
        " [no matches]".to_string()
    } else if !app.history_search_results.is_empty() {
        format!(
            " [{}/{}]",
            app.history_search_selected + 1,
            app.history_search_results.len()
        )
    } else {
        String::new()
    };
    search_spans.push(Span::styled(
        match_info,
        Style::default().fg(theme.search_bar_fg),
    ));

    let search_line = Line::from(search_spans);
    let search_paragraph =
        Paragraph::new(search_line).style(Style::default().bg(theme.input_bg).fg(theme.input_fg));
    frame.render_widget(search_paragraph, rows[0]);

    // Render filtered results.
    let results_height = rows[1].height as usize;
    if results_height == 0 {
        return;
    }
    let max_width = rows[1].width as usize;

    let entries = app.history.entries();
    let results = &app.history_search_results;
    let selected = app.history_search_selected;

    // Window the results around the selected entry.
    let start = if selected >= results_height {
        selected - results_height + 1
    } else {
        0
    };
    let end = (start + results_height).min(results.len());

    let mut result_lines: Vec<Line<'static>> = Vec::new();
    for (display_idx, &entry_idx) in results[start..end].iter().enumerate() {
        let abs_idx = start + display_idx;
        let is_selected = abs_idx == selected;

        if let Some(entry) = entries.get(entry_idx) {
            // Truncate prompt to fit in the available width.
            let prompt_display = entry.prompt.replace('\n', " ");
            let truncated = if prompt_display.len() > max_width.saturating_sub(4) {
                let boundary = prompt_display
                    .char_indices()
                    .take_while(|&(i, _)| i < max_width.saturating_sub(5))
                    .last()
                    .map_or(0, |(i, c)| i + c.len_utf8());
                format!("{}\u{2026}", &prompt_display[..boundary])
            } else {
                prompt_display
            };

            let (prefix_style, text_style) = if is_selected {
                (
                    Style::default()
                        .fg(theme.command_selected_fg)
                        .bg(theme.command_selected_bg)
                        .add_modifier(Modifier::BOLD),
                    Style::default()
                        .fg(theme.command_selected_fg)
                        .bg(theme.command_selected_bg),
                )
            } else {
                (
                    Style::default()
                        .fg(theme.command_match_fg)
                        .add_modifier(Modifier::BOLD),
                    Style::default().fg(theme.command_desc_fg),
                )
            };

            result_lines.push(Line::from(vec![
                Span::styled(if is_selected { " > " } else { "   " }, prefix_style),
                Span::styled(truncated, text_style),
            ]));
        }
    }

    let results_paragraph =
        Paragraph::new(result_lines).style(Style::default().bg(theme.input_bg).fg(theme.input_fg));
    frame.render_widget(results_paragraph, rows[1]);
}

/// Apply search match highlighting to cached lines.
///
/// Returns a new vector of lines with matching substrings highlighted.
/// The current match uses a distinct (brighter) style so the user can
/// see which match is focused. Each match is a `(line_index, start_byte,
/// end_byte)` tuple with byte offsets into the original flattened span text.
fn apply_search_highlights(
    lines: &[Line<'static>],
    matches: &[(usize, usize, usize)],
    current_match_idx: usize,
    theme: &Theme,
) -> Vec<Line<'static>> {
    // Build a lookup: line_index → list of (start_byte, end_byte, is_current).
    let mut match_map: std::collections::HashMap<usize, Vec<(usize, usize, bool)>> =
        std::collections::HashMap::new();
    for (i, &(line_idx, start, end)) in matches.iter().enumerate() {
        match_map
            .entry(line_idx)
            .or_default()
            .push((start, end, i == current_match_idx));
    }

    let highlight_style = Style::default()
        .bg(theme.search_highlight_bg)
        .fg(theme.search_highlight_fg);
    let current_style = Style::default()
        .bg(theme.search_current_bg)
        .fg(theme.search_current_fg)
        .add_modifier(Modifier::BOLD);

    lines
        .iter()
        .enumerate()
        .map(|(idx, line)| {
            if let Some(line_matches) = match_map.get(&idx) {
                highlight_line(line, line_matches, highlight_style, current_style)
            } else {
                line.clone()
            }
        })
        .collect()
}

/// Highlight matching substrings in a single line.
///
/// Operates on the flattened text of all spans, then re-splits into spans
/// preserving original styles for non-matched regions. Each match is a
/// `(start_byte, end_byte, is_current)` tuple with byte offsets into the
/// flattened span text.
fn highlight_line(
    line: &Line<'static>,
    matches: &[(usize, usize, bool)],
    highlight_style: Style,
    current_style: Style,
) -> Line<'static> {
    // Flatten spans into a list of (byte_range, style, text) segments.
    let mut flat: Vec<(usize, usize, Style, String)> = Vec::new();
    let mut offset = 0;
    for span in &line.spans {
        let text = span.content.to_string();
        let len = text.len();
        flat.push((offset, offset + len, span.style, text));
        offset += len;
    }

    if flat.is_empty() {
        return line.clone();
    }

    let total_len = offset;

    // Build a sorted list of highlight ranges.
    let mut highlights: Vec<(usize, usize, Style)> = matches
        .iter()
        .filter_map(|&(start, end, is_current)| {
            if end <= total_len {
                Some((
                    start,
                    end,
                    if is_current {
                        current_style
                    } else {
                        highlight_style
                    },
                ))
            } else {
                None
            }
        })
        .collect();
    highlights.sort_by_key(|&(start, _, _)| start);

    // Re-build spans by walking through the flat segments and splitting
    // where highlights begin/end.
    let mut result_spans: Vec<Span<'static>> = Vec::new();

    for (seg_start, seg_end, seg_style, seg_text) in &flat {
        let mut pos = *seg_start;
        let seg_bytes = seg_text.as_bytes();

        for &(hl_start, hl_end, hl_style) in &highlights {
            // Skip highlights that don't overlap this segment.
            if hl_end <= pos || hl_start >= *seg_end {
                continue;
            }

            // Emit non-highlighted prefix within this segment.
            if hl_start > pos {
                let local_start = pos - seg_start;
                let local_end = hl_start - seg_start;
                if local_end <= seg_bytes.len() {
                    result_spans.push(Span::styled(
                        seg_text[local_start..local_end].to_string(),
                        *seg_style,
                    ));
                }
                pos = hl_start;
            }

            // Emit the highlighted portion within this segment.
            let hl_local_start = pos.saturating_sub(*seg_start);
            let hl_local_end = hl_end.min(*seg_end) - seg_start;
            if hl_local_end <= seg_bytes.len() && hl_local_start < hl_local_end {
                result_spans.push(Span::styled(
                    seg_text[hl_local_start..hl_local_end].to_string(),
                    hl_style,
                ));
            }
            pos = hl_end.min(*seg_end);
        }

        // Emit any remaining non-highlighted suffix.
        if pos < *seg_end {
            let local_start = pos - seg_start;
            result_spans.push(Span::styled(
                seg_text[local_start..].to_string(),
                *seg_style,
            ));
        }
    }

    Line::from(result_spans)
}

// ---------------------------------------------------------------------------
// Toast notification overlay
// ---------------------------------------------------------------------------

/// Maximum number of toasts to display simultaneously.
const MAX_VISIBLE_TOASTS: usize = 3;

/// Render up to [`MAX_VISIBLE_TOASTS`] toast notifications stacked vertically
/// in the top-right corner of the terminal, just below the tab bar.
fn render_toast(app: &App, frame: &mut Frame, area: Rect) {
    if app.toasts.is_empty() {
        return;
    }

    let theme = &app.theme;

    // Show the most recent toasts (newest last in the Vec, rendered top-to-bottom).
    let visible_start = app.toasts.len().saturating_sub(MAX_VISIBLE_TOASTS);
    let visible = &app.toasts[visible_start..];

    // Each toast is 3 rows tall (top border + content + bottom border).
    let toast_height: u16 = 3;
    let mut y = 3_u16.min(area.height.saturating_sub(toast_height));

    for toast in visible.iter().rev() {
        if y + toast_height > area.height {
            break;
        }

        let icon = match toast.kind {
            ToastKind::Success => CHECK_MARK,
            ToastKind::Error => CROSS_MARK,
        };
        let icon_color = match toast.kind {
            ToastKind::Success => theme.toast_success,
            ToastKind::Error => theme.toast_error,
        };

        let content = Line::from(vec![
            Span::styled(
                format!(" {icon} "),
                Style::default().fg(icon_color).add_modifier(Modifier::BOLD),
            ),
            Span::styled(toast.message.as_str(), Style::default().fg(theme.toast_fg)),
            Span::raw(" "),
        ]);

        // Width: icon (3) + message + trailing space + borders (2).
        let text_width = 3 + toast.message.len() + 1;
        let popup_width = (text_width + 2).min(area.width as usize) as u16;

        let x = area.width.saturating_sub(popup_width).saturating_sub(1);
        let popup = Rect::new(x, y, popup_width, toast_height);

        frame.render_widget(Clear, popup);

        let widget = Paragraph::new(content)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .border_style(Style::default().fg(theme.toast_border)),
            )
            .style(Style::default().bg(theme.toast_bg));

        frame.render_widget(widget, popup);
        y += toast_height;
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create the default dark theme for tests.
    fn test_theme() -> Theme {
        Theme::dark()
    }

    // -----------------------------------------------------------------------
    // strip_leading_empty_after_turn_start tests
    // -----------------------------------------------------------------------

    #[test]
    fn strip_empty_preserves_non_turn_start_lines() {
        let lines = vec![
            DisplayLine::Text("hello".into()),
            DisplayLine::Text("world".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let result = strip_leading_empty_after_turn_start(&refs);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn strip_empty_removes_empty_text_after_turn_start() {
        let lines = vec![
            DisplayLine::TurnStart,
            DisplayLine::Text("".into()),
            DisplayLine::Text("".into()),
            DisplayLine::Text("hello".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let result = strip_leading_empty_after_turn_start(&refs);
        // TurnStart + "hello" (two empty lines removed)
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], DisplayLine::TurnStart));
        assert!(matches!(result[1], DisplayLine::Text(ref s) if s == "hello"));
    }

    #[test]
    fn strip_empty_keeps_empty_text_not_after_turn_start() {
        let lines = vec![
            DisplayLine::Text("first".into()),
            DisplayLine::Text("".into()),
            DisplayLine::Text("second".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let result = strip_leading_empty_after_turn_start(&refs);
        assert_eq!(result.len(), 3);
    }

    // -----------------------------------------------------------------------
    // collapse_results tests
    // -----------------------------------------------------------------------

    #[test]
    fn collapse_results_groups_consecutive_tool_results_hidden() {
        let lines = vec![
            DisplayLine::ToolResult {
                content: "abc".into(),
                is_error: false,
            },
            DisplayLine::ToolResult {
                content: "defgh".into(),
                is_error: false,
            },
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let blocks = collapse_results(
            &refs,
            ResultDisplay::Hidden,
            &std::collections::HashMap::new(),
        );

        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            CollapsedBlock::ToolResultRun(ToolResultRun::Hidden { total_bytes }) => {
                assert_eq!(*total_bytes, 8); // "abc" + "defgh"
            }
            other => panic!("expected Hidden, got {:?}", other),
        }
    }

    #[test]
    fn collapse_results_groups_consecutive_tool_results_full() {
        let lines = vec![
            DisplayLine::ToolResult {
                content: "line1\nline2".into(),
                is_error: false,
            },
            DisplayLine::ToolResult {
                content: "line3".into(),
                is_error: true,
            },
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let blocks = collapse_results(
            &refs,
            ResultDisplay::Full,
            &std::collections::HashMap::new(),
        );

        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            CollapsedBlock::ToolResultRun(ToolResultRun::Full { lines }) => {
                assert_eq!(lines.len(), 3);
                assert_eq!(lines[0].text, "line1");
                assert!(!lines[0].is_error);
                assert_eq!(lines[1].text, "line2");
                assert!(!lines[1].is_error);
                assert_eq!(lines[2].text, "line3");
                assert!(lines[2].is_error);
            }
            other => panic!("expected Full, got {:?}", other),
        }
    }

    #[test]
    fn collapse_results_compact_truncates_long_runs() {
        // Create a run with more lines than COMPACT_RESULT_RUN_MAX.
        // Compact should show the *last* 3 lines: "c", "d", "e".
        let lines = vec![DisplayLine::ToolResult {
            content: "a\nb\nc\nd\ne".into(),
            is_error: false,
        }];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let blocks = collapse_results(
            &refs,
            ResultDisplay::Compact,
            &std::collections::HashMap::new(),
        );

        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            CollapsedBlock::ToolResultRun(ToolResultRun::Compact {
                visible,
                hidden_count,
            }) => {
                assert_eq!(visible.len(), 3);
                assert_eq!(visible[0].text, "c");
                assert_eq!(visible[1].text, "d");
                assert_eq!(visible[2].text, "e");
                assert_eq!(*hidden_count, 2);
            }
            other => panic!("expected Compact, got {:?}", other),
        }
    }

    #[test]
    fn collapse_results_compact_shows_last_lines_of_multientry_run() {
        // Two consecutive ToolResult entries form a single run.
        // "first_a\nfirst_b" (2 lines) + "second_a\nsecond_b" (2 lines) = 4 lines total.
        // Compact should show the last 3 lines: "first_b", "second_a", "second_b".
        let lines = vec![
            DisplayLine::ToolResult {
                content: "first_a\nfirst_b".into(),
                is_error: false,
            },
            DisplayLine::ToolResult {
                content: "second_a\nsecond_b".into(),
                is_error: false,
            },
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let blocks = collapse_results(
            &refs,
            ResultDisplay::Compact,
            &std::collections::HashMap::new(),
        );

        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            CollapsedBlock::ToolResultRun(ToolResultRun::Compact {
                visible,
                hidden_count,
            }) => {
                assert_eq!(visible.len(), 3);
                assert_eq!(visible[0].text, "first_b");
                assert_eq!(visible[1].text, "second_a");
                assert_eq!(visible[2].text, "second_b");
                assert_eq!(*hidden_count, 1);
            }
            other => panic!("expected Compact, got {:?}", other),
        }
    }

    #[test]
    fn collapse_results_compact_single_line_shows_it() {
        // A run with exactly one line should show that line with hidden_count 0.
        let lines = vec![DisplayLine::ToolResult {
            content: "only_line".into(),
            is_error: false,
        }];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let blocks = collapse_results(
            &refs,
            ResultDisplay::Compact,
            &std::collections::HashMap::new(),
        );

        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            CollapsedBlock::ToolResultRun(ToolResultRun::Compact {
                visible,
                hidden_count,
            }) => {
                assert_eq!(visible.len(), 1);
                assert_eq!(visible[0].text, "only_line");
                assert_eq!(*hidden_count, 0);
            }
            other => panic!("expected Compact, got {:?}", other),
        }
    }

    #[test]
    fn collapse_results_groups_text_runs() {
        let lines = vec![
            DisplayLine::Text("hello".into()),
            DisplayLine::Text("world".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let blocks = collapse_results(
            &refs,
            ResultDisplay::Full,
            &std::collections::HashMap::new(),
        );

        assert_eq!(blocks.len(), 1);
        match &blocks[0] {
            CollapsedBlock::TextRun(md) => {
                assert_eq!(md, "hello\nworld");
            }
            other => panic!("expected TextRun, got {:?}", other),
        }
    }

    #[test]
    fn collapse_results_single_variants_pass_through() {
        let lines = vec![
            DisplayLine::System("sys".into()),
            DisplayLine::Error("err".into()),
            DisplayLine::TurnStart,
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let blocks = collapse_results(
            &refs,
            ResultDisplay::Full,
            &std::collections::HashMap::new(),
        );

        assert_eq!(blocks.len(), 3);
        assert!(matches!(blocks[0], CollapsedBlock::Single(_)));
        assert!(matches!(blocks[1], CollapsedBlock::Single(_)));
        assert!(matches!(blocks[2], CollapsedBlock::Single(_)));
    }

    #[test]
    fn collapse_results_interleaved_types() {
        let lines = vec![
            DisplayLine::Text("text1".into()),
            DisplayLine::ToolUse {
                tool: "Read".into(),
                input_preview: "file.rs".into(),
            },
            DisplayLine::ToolResult {
                content: "content".into(),
                is_error: false,
            },
            DisplayLine::Text("text2".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let blocks = collapse_results(
            &refs,
            ResultDisplay::Full,
            &std::collections::HashMap::new(),
        );

        // TextRun, ToolUseHeader, ToolResultRun, TextRun
        assert_eq!(blocks.len(), 4);
        assert!(matches!(blocks[0], CollapsedBlock::TextRun(_)));
        assert!(matches!(blocks[1], CollapsedBlock::ToolUseHeader { .. }));
        assert!(matches!(blocks[2], CollapsedBlock::ToolResultRun(_)));
        assert!(matches!(blocks[3], CollapsedBlock::TextRun(_)));
    }

    // -----------------------------------------------------------------------
    // render_to_spans tests
    // -----------------------------------------------------------------------

    #[test]
    fn render_tool_result_hidden_shows_byte_count() {
        let blocks = vec![CollapsedBlock::ToolResultRun(ToolResultRun::Hidden {
            total_bytes: 42,
        })];
        let lines = render_to_spans(&blocks, &test_theme());
        assert_eq!(lines.len(), 1);
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("42 bytes"));
        assert!(text.contains("press 'r'"));
    }

    #[test]
    fn render_tool_result_hidden_formats_kb() {
        let blocks = vec![CollapsedBlock::ToolResultRun(ToolResultRun::Hidden {
            total_bytes: 2048,
        })];
        let lines = render_to_spans(&blocks, &test_theme());
        let text: String = lines[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("2.0 KB"));
    }

    #[test]
    fn render_tool_result_full_shows_all_lines() {
        let blocks = vec![CollapsedBlock::ToolResultRun(ToolResultRun::Full {
            lines: vec![
                ToolResultLine {
                    text: "first",
                    is_error: false,
                },
                ToolResultLine {
                    text: "second",
                    is_error: false,
                },
            ],
        })];
        let output = render_to_spans(&blocks, &test_theme());
        assert_eq!(output.len(), 2);
        // First line should have the connector prefix.
        let first_text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_text.contains(RESULT_CONNECTOR));
        assert!(first_text.contains("first"));
    }

    #[test]
    fn render_tool_result_compact_shows_hidden_count() {
        let blocks = vec![CollapsedBlock::ToolResultRun(ToolResultRun::Compact {
            visible: vec![ToolResultLine {
                text: "last_line",
                is_error: false,
            }],
            hidden_count: 5,
        })];
        let output = render_to_spans(&blocks, &test_theme());
        // 1 visible line + 1 "more lines hidden" line
        assert_eq!(output.len(), 2);
        let last_text: String = output[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(last_text.contains("5 more lines hidden"));
    }

    #[test]
    fn render_tool_result_compact_no_hidden_line_when_zero() {
        let blocks = vec![CollapsedBlock::ToolResultRun(ToolResultRun::Compact {
            visible: vec![ToolResultLine {
                text: "only",
                is_error: false,
            }],
            hidden_count: 0,
        })];
        let output = render_to_spans(&blocks, &test_theme());
        assert_eq!(output.len(), 1);
    }

    #[test]
    fn render_text_run_adds_block_marker() {
        let blocks = vec![CollapsedBlock::TextRun("hello world".into())];
        let output = render_to_spans(&blocks, &test_theme());
        assert!(!output.is_empty());
        let first_text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_text.contains(BLOCK_MARKER));
        assert!(first_text.contains("hello world"));
    }

    #[test]
    fn render_single_system_line() {
        let dl = DisplayLine::System("system msg".into());
        let blocks = vec![CollapsedBlock::Single(&dl)];
        let output = render_to_spans(&blocks, &test_theme());
        assert_eq!(output.len(), 1);
        let text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("system msg"));
    }

    #[test]
    fn render_single_error_line() {
        let dl = DisplayLine::Error("error msg".into());
        let blocks = vec![CollapsedBlock::Single(&dl)];
        let output = render_to_spans(&blocks, &test_theme());
        assert_eq!(output.len(), 1);
        let text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("error msg"));
    }

    // -----------------------------------------------------------------------
    // Full pipeline (collapse_tool_results) integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn full_pipeline_empty_input() {
        let lines: Vec<&DisplayLine> = vec![];
        let no_overrides = std::collections::HashMap::new();
        let result =
            collapse_tool_results(&lines, ResultDisplay::Full, &no_overrides, &test_theme());
        assert!(result.is_empty());
    }

    #[test]
    fn full_pipeline_hidden_mode_replaces_results() {
        let lines = vec![
            DisplayLine::ToolResult {
                content: "abc".into(),
                is_error: false,
            },
            DisplayLine::ToolResult {
                content: "def".into(),
                is_error: false,
            },
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let no_overrides = std::collections::HashMap::new();
        let output =
            collapse_tool_results(&refs, ResultDisplay::Hidden, &no_overrides, &test_theme());
        assert_eq!(output.len(), 1);
        let text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("6 bytes"));
    }

    #[test]
    fn full_pipeline_strips_empty_after_turn_start() {
        let lines = vec![
            DisplayLine::TurnStart,
            DisplayLine::Text("".into()),
            DisplayLine::Text("content".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let no_overrides = std::collections::HashMap::new();
        let output =
            collapse_tool_results(&refs, ResultDisplay::Full, &no_overrides, &test_theme());
        // TurnStart produces 1 empty line, then "content" produces 1 line with marker.
        // The empty Text("") should be stripped.
        let all_text: String = output
            .iter()
            .flat_map(|l| l.spans.iter().map(|s| s.content.as_ref().to_string()))
            .collect::<Vec<_>>()
            .join("");
        assert!(all_text.contains("content"));
    }

    #[test]
    fn collapse_results_respects_section_override() {
        let lines = vec![
            DisplayLine::ToolUse {
                tool: "Read".into(),
                input_preview: "file.rs".into(),
            },
            DisplayLine::ToolResult {
                content: "line1\nline2\nline3".into(),
                is_error: false,
            },
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();

        // Global mode is Full, but override section 0 to Hidden.
        let mut overrides = std::collections::HashMap::new();
        overrides.insert(0, ResultDisplay::Hidden);

        let blocks = collapse_results(&refs, ResultDisplay::Full, &overrides);

        // Should have ToolUseHeader + ToolResultRun.
        assert_eq!(blocks.len(), 2);
        match &blocks[0] {
            CollapsedBlock::ToolUseHeader { effective_mode, .. } => {
                assert_eq!(*effective_mode, ResultDisplay::Hidden);
            }
            other => panic!("expected ToolUseHeader, got {:?}", other),
        }
        match &blocks[1] {
            CollapsedBlock::ToolResultRun(ToolResultRun::Hidden { total_bytes }) => {
                assert_eq!(*total_bytes, 17); // "line1\nline2\nline3"
            }
            other => panic!("expected Hidden ToolResultRun, got {:?}", other),
        }
    }
}
