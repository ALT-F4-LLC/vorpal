//! TUI widget rendering.
//!
//! Builds the ratatui widget tree from application state: tab bar, content
//! area with scrollable output, status bar, and help overlay.

use super::manager::ClaudeOptions;
use super::state::{
    self, AgentActivity, AgentState, AgentStatus, App, DiffLine, DisplayLine, InputField,
    InputMode, ResultDisplay, SplitPane, ToastKind, COMMANDS, EFFORT_LEVELS, MODELS,
    PERMISSION_MODES,
};
use super::theme::Theme;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

// ---------------------------------------------------------------------------
// Unicode constants
// ---------------------------------------------------------------------------

const SPINNER_FRAMES: &[&str] = &["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
const ARROW_RIGHT: &str = "▶";
const CHECK_MARK: &str = "✔";
const CROSS_MARK: &str = "✘";
const CIRCLE: &str = "●";
pub(super) const BLOCK_MARKER: &str = "*";
pub(super) const RESULT_CONNECTOR: &str = "⎿";
pub(super) const SESSION_MARKER: &str = "✻";

/// Indentation that matches the width of `* ` (marker + space).
const CONTINUATION_INDENT: &str = "  ";

/// Minimum terminal width required for the TUI.
const MIN_WIDTH: u16 = 40;
/// Minimum terminal height required for the TUI.
const MIN_HEIGHT: u16 = 12;

// ---------------------------------------------------------------------------
// Main render entry point
// ---------------------------------------------------------------------------

/// Render the entire TUI from application state.
///
/// Splits the terminal into four vertical sections (tab bar, content area,
/// inline input area, status bar) and draws each one. The inline input area
/// is hidden when no agents exist. If the help overlay is active it is drawn
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

    // Compute inline input area height dynamically from the chat text content.
    // Hidden on the welcome screen; otherwise 1 row for the top border plus
    // enough content rows to fit the wrapped text, clamped to 3..=10 total
    // (with a 40% terminal height cap).
    let input_height: u16 = if app.agents.is_empty() {
        0
    } else {
        let content_lines = count_wrapped_lines(app.chat_input.text(), area.width as usize);
        // border (1) + content rows (at least 2 so the input doesn't feel cramped)
        let desired = 1 + content_lines.max(2) as u16;
        // Hard cap at 10 rows, but also respect a 40% terminal height limit.
        let max_from_percent = (area.height * 40 / 100).max(3);
        desired.clamp(3, 10.min(max_from_percent))
    };

    let chunks = Layout::vertical([
        Constraint::Length(2),            // tab bar
        Constraint::Fill(1),              // content
        Constraint::Length(input_height), // inline input area
        Constraint::Length(2),            // status bar
    ])
    .split(area);

    // Store the content area rect for mouse scroll hit-testing.
    app.content_rect = Some(chunks[1]);
    // Store the inline input area rect.
    app.chat_input_rect = chunks[2];
    // Reset jump-to-bottom indicator rect; render_content / render_content_pane
    // will set it when the indicator is visible.
    app.jump_to_bottom_rect = None;

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

    render_inline_input(app, frame, chunks[2]);
    render_status(app, frame, chunks[3]);

    if app.show_help {
        render_help(&app.theme, frame, area);
    }

    if app.show_graph {
        render_graph(app, frame, area);
    }

    if app.show_dashboard {
        render_dashboard(app, frame, area);
    }

    if app.confirm_close {
        render_confirm_close(app, frame, area);
    }

    if app.input_mode == InputMode::TemplatePicker {
        render_template_picker(app, frame, area);
    }

    if app.input_mode == InputMode::SessionPicker {
        render_session_picker(app, frame, area);
    }

    if app.input_mode == InputMode::Input {
        render_input(app, frame, area);
    }

    if app.input_mode == InputMode::Settings {
        render_settings(app, frame, area);
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
        let dim = Style::default().fg(theme.tab_no_agents);
        let key = Style::default()
            .fg(theme.tab_highlight)
            .add_modifier(Modifier::BOLD);
        let hint = Paragraph::new(Line::from(vec![
            Span::styled(" No agents -- press ", dim),
            Span::styled("n", key),
            Span::styled(" to create one · ", dim),
            Span::styled("?", key),
            Span::styled(" help · ", dim),
            Span::styled("q", key),
            Span::styled(" quit", dim),
        ]))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.tab_border)),
        );
        frame.render_widget(hint, area);
        return;
    }

    // Show the activity label on the focused tab only when the tab bar has
    // enough room. A rough heuristic: at least 25 columns per agent ensures
    // the extra ~12 chars of activity text won't push tabs off-screen.
    let show_focused_activity =
        !app.agents.is_empty() && (area.width as usize / app.agents.len()) >= 25;

    let all_titles: Vec<Line<'_>> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let label = workspace_label(&agent.workspace);
            let num = i + 1;
            let (badge, badge_color) = match (&agent.status, &agent.activity) {
                (&AgentStatus::Pending, _) => ("\u{25CB}", theme.tab_badge_idle), // ○
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
                Span::raw(format!(" {num}: {label} ")),
            ];
            // For the focused tab, append a short activity label when the
            // agent is actively working (thinking or running a tool).
            if show_focused_activity && i == app.focused {
                let activity_text = match &agent.activity {
                    AgentActivity::Thinking => Some("thinking".to_string()),
                    AgentActivity::Tool(name) => Some(truncate_chars(name, 10, "\u{2026}")),
                    _ => None,
                };
                if let Some(text) = activity_text {
                    spans.push(Span::styled(
                        format!("({text}) "),
                        Style::default()
                            .fg(theme.tab_border)
                            .add_modifier(Modifier::DIM),
                    ));
                }
            }
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
                .add_modifier(Modifier::BOLD),
        )
        .style(Style::default().fg(theme.tab_text))
        .divider(Span::styled("│", Style::default().fg(theme.tab_border)))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(theme.tab_border)),
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

/// Truncate a string to at most `max_chars` **characters** (not bytes), appending
/// `suffix` when truncation occurs. This is UTF-8 safe — it never splits a
/// multi-byte character.
fn truncate_chars(s: &str, max_chars: usize, suffix: &str) -> String {
    if s.chars().count() <= max_chars {
        return s.to_string();
    }
    let truncated: String = s.chars().take(max_chars).collect();
    format!("{truncated}{suffix}")
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
// Content area
// ---------------------------------------------------------------------------

/// Render a centered placeholder message for pending agents.
fn render_pending_placeholder(theme: &Theme, frame: &mut Frame, area: Rect, block: Block<'_>) {
    let msg = Paragraph::new(Line::from(Span::styled(
        "Type a message below to start this agent",
        Style::default()
            .fg(theme.content_empty)
            .add_modifier(Modifier::DIM),
    )))
    .alignment(Alignment::Center)
    .block(block);
    frame.render_widget(msg, area);
}

/// Render the main content area with the focused agent's output.
fn render_content(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::NONE);

    let inner = block.inner(area);
    let theme = &app.theme;

    match app.focused_agent() {
        None => {
            // Empty content area — quick start hints are in the tab bar.
            frame.render_widget(block, area);
        }
        Some(_) => {
            let height = inner.height as usize;
            let focused = app.focused;
            let agent = &app.agents[focused];

            if matches!(agent.status, AgentStatus::Pending) {
                render_pending_placeholder(theme, frame, area, block);
                return;
            }

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
            let evicted = agent.evicted_line_count;

            let max_scroll = total_rows.saturating_sub(height);

            // scroll_offset 0 = pinned to bottom (latest output).
            // Convert to scroll-from-top for Paragraph::scroll().
            let clamped_offset = agent.scroll_offset.min(max_scroll);
            let scroll_y = max_scroll.saturating_sub(clamped_offset) as u16;

            // Apply search highlighting if there are active search matches.
            // Otherwise, borrow from the cache to avoid deep-cloning every
            // Span/String each frame.
            let mut display_lines =
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

            maybe_prepend_truncation_notice(
                &mut display_lines,
                evicted,
                clamped_offset,
                max_scroll,
                &app.theme,
            );

            append_streaming_cursor(
                &mut display_lines,
                agent,
                app.tick,
                app.theme.streaming_cursor,
            );

            append_thinking_indicator(&mut display_lines, agent, app.tick, &app.theme);

            let paragraph = Paragraph::new(display_lines)
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((scroll_y, 0));

            frame.render_widget(paragraph, area);

            // Floating jump-to-bottom indicator when scrolled up with new output.
            let has_new = app.agents[focused].has_new_output;
            let scroll_off = app.agents[focused].scroll_offset;
            maybe_render_jump_to_bottom(app, frame, inner, has_new, scroll_off, true);
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

/// Prepend a dim truncation notice when lines have been evicted and the
/// viewport includes the very top of the buffer.
///
/// The notice appears only when `evicted > 0` and the user has scrolled to
/// the maximum offset (`clamped_offset == max_scroll`), meaning the first
/// line of the ring buffer is visible.
fn maybe_prepend_truncation_notice<'a>(
    lines: &mut Vec<Line<'a>>,
    evicted: usize,
    clamped_offset: usize,
    max_scroll: usize,
    theme: &Theme,
) {
    if evicted == 0 {
        return;
    }
    // The viewport includes the first buffer line when the user has scrolled
    // all the way to the top (clamped_offset == max_scroll).
    if clamped_offset != max_scroll {
        return;
    }
    let notice = format!("(earlier output truncated -- {} lines removed)", evicted);
    let style = Style::default()
        .fg(theme.system_text)
        .add_modifier(Modifier::ITALIC | Modifier::DIM);
    lines.insert(0, Line::from(Span::styled(notice, style)));
}

/// Append a blinking cursor indicator when the agent is actively generating
/// and the viewport is pinned to the bottom.
fn append_streaming_cursor<'a>(
    lines: &mut Vec<Line<'a>>,
    agent: &AgentState,
    tick: usize,
    color: Color,
) {
    let is_streaming = matches!(
        agent.activity,
        AgentActivity::Thinking | AgentActivity::Tool(_)
    );
    if is_streaming && agent.scroll_offset == 0 {
        let cursor_char = if tick % 6 < 3 { "█" } else { " " };
        let cursor_span = Span::styled(cursor_char, Style::default().fg(color));
        if let Some(last_line) = lines.last_mut() {
            last_line.spans.push(cursor_span);
        } else {
            lines.push(Line::from(cursor_span));
        }
    }
}

/// Append a synthetic thinking indicator with a braille spinner when the agent
/// is in the `Thinking` state but no thinking content has arrived yet (the
/// brief pause before the first thinking token).
///
/// The indicator line uses the tilde marker (`~ `) followed by an animated
/// spinner frame, styled with `thinking_color` in dim+italic to match the
/// thinking block style.
fn append_thinking_indicator<'a>(
    lines: &mut Vec<Line<'a>>,
    agent: &AgentState,
    tick: usize,
    theme: &Theme,
) {
    if !matches!(agent.activity, AgentActivity::Thinking) {
        return;
    }
    // Only show when no thinking content has arrived yet — i.e. the last
    // output line is NOT a Thinking variant.
    let has_thinking = agent
        .output
        .back()
        .is_some_and(|dl| matches!(dl, DisplayLine::Thinking(_)));
    if has_thinking {
        return;
    }
    let frame_idx = tick % SPINNER_FRAMES.len();
    let spinner = SPINNER_FRAMES[frame_idx];
    lines.push(Line::from(vec![
        Span::styled("~ ", Style::default().fg(theme.thinking_color)),
        Span::styled(
            spinner.to_string(),
            Style::default()
                .fg(theme.thinking_color)
                .add_modifier(Modifier::DIM | Modifier::ITALIC),
        ),
    ]));
}

/// Conditionally render the jump-to-bottom indicator when scrolled up.
/// Clears the stored rect when the indicator is not shown.
fn maybe_render_jump_to_bottom(
    app: &mut App,
    frame: &mut Frame,
    inner: Rect,
    has_new_output: bool,
    scroll_offset: usize,
    is_focused: bool,
) {
    if is_focused && scroll_offset > 0 {
        render_jump_to_bottom(app, frame, inner, has_new_output);
    } else if is_focused {
        app.jump_to_bottom_rect = None;
    }
}

/// Render a floating "jump to bottom" indicator at the bottom-center of the
/// content area. Shown when the user is scrolled up, prompting them to press
/// G or click the indicator to jump to the latest output. When new output has
/// arrived the arrow pulses to draw attention; otherwise a static style is used.
fn render_jump_to_bottom(
    app: &mut App,
    frame: &mut Frame,
    content_area: Rect,
    has_new_output: bool,
) {
    let theme = &app.theme;

    let arrow_style = if has_new_output {
        // Pulsing arrow: alternate bold/dim to draw attention to new output.
        if app.tick % 4 < 2 {
            Style::default()
                .fg(theme.jump_to_bottom_fg)
                .bg(theme.jump_to_bottom_bg)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(theme.jump_to_bottom_fg)
                .bg(theme.jump_to_bottom_bg)
                .add_modifier(Modifier::DIM)
        }
    } else {
        // Static style when simply scrolled up without new output.
        Style::default()
            .fg(theme.jump_to_bottom_fg)
            .bg(theme.jump_to_bottom_bg)
    };

    let label_text = if has_new_output {
        " ↓ New output — press G to jump to bottom "
    } else {
        " ↓ Press G to jump to bottom "
    };
    let label_width = label_text.width() as u16;

    // The indicator is 1 row tall and centered horizontally, positioned at
    // the bottom of the content area (2 rows up so it doesn't overlap the
    // very last visible line).
    let indicator_width = (label_width + 2).min(content_area.width);
    let indicator_x = content_area.x + content_area.width.saturating_sub(indicator_width) / 2;
    let indicator_y = content_area.y + content_area.height.saturating_sub(2);

    let indicator_rect = Rect::new(indicator_x, indicator_y, indicator_width, 1);

    // Store the rect for mouse click hit-testing.
    app.jump_to_bottom_rect = Some(indicator_rect);

    // Clear the area under the indicator so it floats above content.
    frame.render_widget(Clear, indicator_rect);

    let indicator = Paragraph::new(Line::from(vec![Span::styled(label_text, arrow_style)]))
        .alignment(Alignment::Center)
        .style(
            Style::default()
                .bg(theme.jump_to_bottom_bg)
                .fg(theme.jump_to_bottom_fg),
        );

    frame.render_widget(indicator, indicator_rect);
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
            inner_width,
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

    if matches!(agent.status, AgentStatus::Pending) {
        render_pending_placeholder(theme, frame, area, block);
        return;
    }

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
    let evicted = agent.evicted_line_count;

    let max_scroll = total_rows.saturating_sub(height);
    let clamped_offset = agent.scroll_offset.min(max_scroll);
    let scroll_y = max_scroll.saturating_sub(clamped_offset) as u16;

    let mut display_lines = borrow_cached_lines(cached);

    maybe_prepend_truncation_notice(
        &mut display_lines,
        evicted,
        clamped_offset,
        max_scroll,
        &app.theme,
    );

    append_streaming_cursor(
        &mut display_lines,
        agent,
        app.tick,
        app.theme.streaming_cursor,
    );

    append_thinking_indicator(&mut display_lines, agent, app.tick, &app.theme);

    let paragraph = Paragraph::new(display_lines)
        .block(block)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y, 0));

    frame.render_widget(paragraph, area);

    // Floating jump-to-bottom indicator when scrolled up with new output.
    let has_new = app.agents[agent_idx].has_new_output;
    let scroll_off = app.agents[agent_idx].scroll_offset;
    maybe_render_jump_to_bottom(app, frame, inner, has_new, scroll_off, is_focused);
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
/// 1. [`strip_empty_text_lines`] — remove parser-emitted blank lines
/// 2. [`collapse_results`] — group lines into [`CollapsedBlock`]s
/// 3. [`render_to_spans`] — convert blocks into styled ratatui [`Line`]s
fn collapse_tool_results<'a>(
    lines: &[&'a DisplayLine],
    mode: ResultDisplay,
    section_overrides: &std::collections::HashMap<usize, ResultDisplay>,
    theme: &Theme,
    content_width: u16,
) -> Vec<Line<'a>> {
    let stripped = strip_empty_text_lines(lines);
    let blocks = collapse_results(&stripped, mode, section_overrides);
    render_to_spans(&blocks, theme, content_width)
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
    /// An inline diff result from an Edit tool.
    DiffBlock {
        diff_ops: &'a [DiffLine],
        file_path: &'a str,
        effective_mode: ResultDisplay,
    },
    /// A run of consecutive thinking lines, collapsed into a summary.
    ThinkingRun {
        /// Number of lines in the collapsed thinking block.
        line_count: usize,
    },
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
/// When `has_diff` is true and `mode` is [`ResultDisplay::Diff`], the result
/// content is hidden (replaced by a DiffBlock). When `has_diff` is false,
/// Diff mode falls through to Compact so non-Edit tools still show output.
///
/// Returns the constructed run and the new index after the last consumed
/// `ToolResult` line. Both the ToolUse-preceded path and the orphan path
/// in [`collapse_results`] delegate to this helper.
fn consume_tool_result_run<'a>(
    lines: &[&'a DisplayLine],
    start: usize,
    mode: ResultDisplay,
    has_diff: bool,
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

    // In Diff mode, only hide results when a DiffBlock was actually emitted
    // (i.e. the tool was an Edit). For non-Edit tools, fall through to Compact.
    let effective_mode = if mode == ResultDisplay::Diff && has_diff {
        ResultDisplay::Hidden
    } else if mode == ResultDisplay::Diff {
        ResultDisplay::Compact
    } else {
        mode
    };

    let run = match effective_mode {
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
        ResultDisplay::Full | ResultDisplay::Diff => ToolResultRun::Full {
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

            // Consume a DiffResult if present (emitted by the parser for Edit tools).
            let mut has_diff = false;
            if i < lines.len() {
                if let DisplayLine::DiffResult {
                    diff_ops,
                    file_path,
                } = lines[i]
                {
                    blocks.push(CollapsedBlock::DiffBlock {
                        diff_ops,
                        file_path,
                        effective_mode,
                    });
                    has_diff = true;
                    i += 1;
                }
            }

            // Now consume the following ToolResult run (if any).
            if i < lines.len() && matches!(lines[i], DisplayLine::ToolResult { .. }) {
                let (run, new_i) = consume_tool_result_run(lines, i, effective_mode, has_diff);
                i = new_i;
                blocks.push(CollapsedBlock::ToolResultRun(run));
            }
        } else if matches!(lines[i], DisplayLine::ToolResult { .. }) {
            // Orphan ToolResult without a preceding ToolUse — use global mode.
            let (run, new_i) = consume_tool_result_run(lines, i, global_mode, false);
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
        } else if matches!(lines[i], DisplayLine::Thinking(_)) {
            let run_start = i;
            while i < lines.len() && matches!(lines[i], DisplayLine::Thinking(_)) {
                i += 1;
            }
            let full_text: String = lines[run_start..i]
                .iter()
                .filter_map(|dl| match dl {
                    DisplayLine::Thinking(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n");
            let line_count = full_text.lines().count().max(1);
            blocks.push(CollapsedBlock::ThinkingRun { line_count });
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

/// Semantic category of a [`CollapsedBlock`], used by the spacing logic in
/// [`render_to_spans`] to decide whether to insert blank lines between
/// adjacent blocks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockCategory {
    TurnStart,
    UserPrompt,
    AssistantText,
    ToolCall,
    ToolResult,
    DiffBlock,
    Thinking,
    TurnSummary,
    System,
    Error,
    /// Catch-all for block types that don't need special spacing rules
    /// (e.g. Result, Stderr, AgentMessage, DiffResult).
    Other,
}

/// Classify a [`CollapsedBlock`] into a [`BlockCategory`] for spacing decisions.
fn categorize_block(block: &CollapsedBlock<'_>) -> BlockCategory {
    match block {
        CollapsedBlock::TextRun(_) => BlockCategory::AssistantText,
        CollapsedBlock::ToolUseHeader { .. } => BlockCategory::ToolCall,
        CollapsedBlock::ToolResultRun(_) => BlockCategory::ToolResult,
        CollapsedBlock::DiffBlock { .. } => BlockCategory::DiffBlock,
        CollapsedBlock::ThinkingRun { .. } => BlockCategory::Thinking,
        CollapsedBlock::Single(dl) => match dl {
            DisplayLine::TurnStart { .. } => BlockCategory::TurnStart,
            DisplayLine::UserPrompt { .. } => BlockCategory::UserPrompt,
            DisplayLine::TurnSummary { .. } => BlockCategory::TurnSummary,
            DisplayLine::System(_) => BlockCategory::System,
            DisplayLine::Error(_) => BlockCategory::Error,
            _ => BlockCategory::Other,
        },
    }
}

/// Determine whether a blank line should be inserted before `current` given
/// that `prev` was the most recently emitted block.
///
/// Implements the spacing rules from the design spec Section 3:
///
/// 1. After TurnStart → one blank line (handled by TurnStart's own renderer).
/// 2. Before/after UserPrompt → one blank line above and below.
/// 3. Between consecutive assistant text blocks → no blank line.
/// 4. Before a tool call → one blank line if preceded by assistant text.
/// 5. After a tool result run → one blank line before the next element.
/// 6. Before TurnSummary → one blank line.
/// 7. Thinking blocks → one blank line above and below.
/// 8. System messages → no blank lines.
/// 9. Errors → one blank line above, no blank line below.
fn needs_blank_line_before(prev: BlockCategory, current: BlockCategory) -> bool {
    // Rule 1: TurnStart already embeds its own blank lines (before and after),
    // so nothing following it needs an additional blank, and TurnStart itself
    // should not get a blank before it from this logic.
    if current == BlockCategory::TurnStart {
        return false;
    }
    if prev == BlockCategory::TurnStart {
        // TurnStart already emits a trailing blank line; no extra needed.
        return false;
    }

    // Rule 8: System messages — no blank lines around them.
    if current == BlockCategory::System || prev == BlockCategory::System {
        return false;
    }

    // Rule 2: Before a user prompt → one blank line above.
    if current == BlockCategory::UserPrompt {
        return true;
    }
    // Rule 2: After a user prompt → one blank line below.
    if prev == BlockCategory::UserPrompt {
        return true;
    }

    // Rule 3: Between consecutive assistant text blocks → no blank line.
    if prev == BlockCategory::AssistantText && current == BlockCategory::AssistantText {
        return false;
    }

    // Rule 4: Before a tool call → one blank line if preceded by assistant text.
    if current == BlockCategory::ToolCall && prev == BlockCategory::AssistantText {
        return true;
    }

    // Rule 5: After a tool result run → one blank line before the next element.
    // Also applies after DiffBlock (which is part of the tool result area).
    if prev == BlockCategory::ToolResult || prev == BlockCategory::DiffBlock {
        return true;
    }

    // Rule 6: Before turn summary → one blank line.
    if current == BlockCategory::TurnSummary {
        return true;
    }

    // Rule 7: Thinking blocks → one blank line above and below.
    if current == BlockCategory::Thinking || prev == BlockCategory::Thinking {
        return true;
    }

    // Rule 9: Errors → one blank line above, no blank line below.
    if current == BlockCategory::Error {
        return true;
    }
    // (After an error, no blank line — so we do not add a rule for prev == Error.)

    false
}

/// Convert a sequence of [`CollapsedBlock`]s into styled ratatui [`Line`]s.
///
/// Applies inter-element spacing rules (design spec Section 3) by inserting
/// blank lines between blocks based on their semantic categories. A final
/// dedup pass ensures no consecutive blank lines appear in the output.
fn render_to_spans<'a>(
    blocks: &[CollapsedBlock<'a>],
    theme: &Theme,
    content_width: u16,
) -> Vec<Line<'a>> {
    let mut out: Vec<Line<'a>> = Vec::new();
    let mut prev_category: Option<BlockCategory> = None;

    for block in blocks {
        let current_category = categorize_block(block);

        // Insert inter-element blank line if the spacing rules require it.
        if let Some(prev) = prev_category {
            if needs_blank_line_before(prev, current_category) {
                out.push(Line::from(""));
            }
        }

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
                    content_width,
                ));
            }
            CollapsedBlock::DiffBlock {
                diff_ops,
                file_path,
                effective_mode,
            } => {
                render_diff_block(diff_ops, file_path, *effective_mode, &mut out, theme);
            }
            CollapsedBlock::TextRun(markdown) => {
                render_text_run(markdown, &mut out, theme);
            }
            CollapsedBlock::ThinkingRun { line_count, .. } => {
                let summary = format!("~ Thinking... ({line_count} lines)");
                out.push(Line::from(Span::styled(
                    summary,
                    Style::default()
                        .fg(theme.thinking_collapsed_fg)
                        .add_modifier(Modifier::DIM | Modifier::ITALIC),
                )));
            }
            CollapsedBlock::Single(dl) => {
                out.extend(display_line_to_lines(dl, theme, content_width));
            }
        }

        prev_category = Some(current_category);
    }

    // Final dedup: collapse consecutive blank lines into at most one.
    dedup_blank_lines(out)
}

/// Remove consecutive blank lines, keeping at most one blank line between
/// non-blank content. Also trims leading blank lines from the output.
fn dedup_blank_lines(lines: Vec<Line<'_>>) -> Vec<Line<'_>> {
    let mut out = Vec::with_capacity(lines.len());
    let mut prev_blank = true; // treat start-of-output as "after a blank" to trim leading blanks
    for line in lines {
        let is_blank = line.spans.is_empty()
            || (line.spans.len() == 1 && line.spans[0].content.as_ref().is_empty());
        if is_blank && prev_blank {
            continue; // skip consecutive blank lines
        }
        prev_blank = is_blank;
        out.push(line);
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
            out.push(Line::from(vec![
                Span::styled(
                    "  │ ".to_string(),
                    Style::default().fg(theme.tool_block_border),
                ),
                Span::styled(
                    format!("{size} (press 'r' to cycle view)"),
                    Style::default().fg(theme.tool_result_hidden),
                ),
            ]));
        }
        ToolResultRun::Compact {
            visible,
            hidden_count,
        } => {
            for trl in visible.iter() {
                out.extend(render_tool_result(
                    trl.text,
                    trl.is_error,
                    ResultDisplay::Compact,
                    theme,
                ));
            }
            if *hidden_count > 0 {
                out.push(Line::from(vec![
                    Span::styled(
                        "  │ ".to_string(),
                        Style::default().fg(theme.tool_block_border),
                    ),
                    Span::styled(
                        format!("... {hidden_count} more lines hidden (press 'r' to cycle view)"),
                        Style::default().fg(theme.tool_result_hidden),
                    ),
                ]));
            }
        }
        ToolResultRun::Full { lines } => {
            for trl in lines.iter() {
                out.extend(render_tool_result(
                    trl.text,
                    trl.is_error,
                    ResultDisplay::Full,
                    theme,
                ));
            }
        }
    }
}

/// Render an inline diff block from pre-computed [`DiffLine`] operations.
///
/// When `effective_mode` is [`ResultDisplay::Diff`], produces a unified-diff-style
/// output with deletions in red and additions in green. In other modes, the diff
/// block is silently skipped (the accompanying ToolResult handles display).
fn render_diff_block<'a>(
    diff_ops: &[DiffLine],
    file_path: &str,
    effective_mode: ResultDisplay,
    out: &mut Vec<Line<'a>>,
    theme: &Theme,
) {
    if effective_mode != ResultDisplay::Diff {
        return;
    }

    let border_prefix = Span::styled(
        "  │ ".to_string(),
        Style::default().fg(theme.tool_block_border),
    );

    // Header line showing the file path.
    out.push(Line::from(vec![
        border_prefix.clone(),
        Span::styled(
            format!("diff {file_path}"),
            Style::default()
                .fg(theme.diff_header_fg)
                .add_modifier(Modifier::BOLD),
        ),
    ]));

    // NOTE: Line numbers are relative to the diff block (starting at 1), not
    // absolute file positions. The `DiffLine` type doesn't carry hunk offset
    // info, so we can't map back to the original file line numbers.
    let mut old_line: usize = 1;
    let mut new_line: usize = 1;

    for op in diff_ops {
        match op {
            DiffLine::Equal(line) => {
                let gutter = format!("{:>4} {:>4} ", old_line, new_line);
                out.push(Line::from(vec![
                    border_prefix.clone(),
                    Span::styled(gutter, Style::default().fg(theme.diff_gutter_fg)),
                    Span::styled(
                        format!("  {line}"),
                        Style::default().fg(theme.diff_context_fg),
                    ),
                ]));
                old_line += 1;
                new_line += 1;
            }
            DiffLine::Delete(line) => {
                let gutter = format!("{:>4}      ", old_line);
                out.push(Line::from(vec![
                    border_prefix.clone(),
                    Span::styled(gutter, Style::default().fg(theme.diff_gutter_fg)),
                    Span::styled(
                        format!("- {line}"),
                        Style::default()
                            .fg(theme.diff_deletion_fg)
                            .bg(theme.diff_deletion_bg),
                    ),
                ]));
                old_line += 1;
            }
            DiffLine::Insert(line) => {
                let gutter = format!("     {:>4} ", new_line);
                out.push(Line::from(vec![
                    border_prefix.clone(),
                    Span::styled(gutter, Style::default().fg(theme.diff_gutter_fg)),
                    Span::styled(
                        format!("+ {line}"),
                        Style::default()
                            .fg(theme.diff_addition_fg)
                            .bg(theme.diff_addition_bg),
                    ),
                ]));
                new_line += 1;
            }
        }
    }

    // Summary line.
    let additions = diff_ops
        .iter()
        .filter(|op| matches!(op, DiffLine::Insert(_)))
        .count();
    let deletions = diff_ops
        .iter()
        .filter(|op| matches!(op, DiffLine::Delete(_)))
        .count();
    out.push(Line::from(vec![
        border_prefix,
        Span::styled(
            format!("+{additions}"),
            Style::default().fg(theme.diff_addition_fg),
        ),
        Span::styled(
            format!(" -{deletions}"),
            Style::default().fg(theme.diff_deletion_fg),
        ),
    ]));
}

/// Render a markdown text run into output lines with block marker prefixes.
fn render_text_run<'a>(markdown: &str, out: &mut Vec<Line<'a>>, theme: &Theme) {
    let mut rendered = super::markdown::render_markdown(markdown, theme);
    // Strip trailing blank lines — the markdown renderer adds a blank line
    // after each paragraph, but inter-block spacing is handled by the
    // spacing pass in `render_to_spans`, so trailing blanks would cause
    // double-spacing.
    while rendered.last().is_some_and(|l| l.width() == 0) {
        rendered.pop();
    }
    // Prefix only the very first non-empty line with a marker.
    // All subsequent lines (including those after blank lines) use
    // continuation indentation. This matches Claude Code's own
    // rendering style where each assistant text block gets a single
    // leading marker.
    let mut need_marker = true;
    for line in rendered {
        if line.width() == 0 {
            out.push(Line::from(""));
        } else if need_marker {
            need_marker = false;
            let marker = theme.marker_char;
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                format!("{marker} "),
                Style::default().fg(theme.block_marker),
            )];
            spans.extend(line.spans);
            out.push(Line::from(spans));
        } else {
            let mut spans: Vec<Span<'static>> = vec![Span::raw(CONTINUATION_INDENT)];
            spans.extend(line.spans);
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
fn render_tool_result(
    content: &str,
    is_error: bool,
    mode: ResultDisplay,
    theme: &Theme,
) -> Vec<Line<'static>> {
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

    let border_color = if is_error {
        theme.tool_block_error_border
    } else {
        theme.tool_block_border
    };
    let prefix = Span::styled("  │ ".to_string(), Style::default().fg(border_color));

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

/// Remove empty `Text("")` lines from the input sequence.
///
/// Parser-emitted empty text lines (e.g. from Claude's `"\n\n"` deltas) are
/// stripped entirely — the `render_to_spans` spacing pass is responsible for
/// all inter-element blank lines, so parser blanks would create unwanted
/// double-spacing.
fn strip_empty_text_lines<'a>(lines: &[&'a DisplayLine]) -> Vec<&'a DisplayLine> {
    lines
        .iter()
        .filter(|dl| !matches!(dl, DisplayLine::Text(s) if s.is_empty()))
        .copied()
        .collect()
}

/// Collapse indicator characters.
const CHEVRON_EXPANDED: &str = "v";
const CHEVRON_COLLAPSED: &str = ">";

/// Render a ToolUse header line with a collapse/expand indicator.
///
/// Shows a chevron (v for full/expanded, > for compact/hidden/collapsed) after
/// the tool name to indicate the display state of the following result section.
fn display_line_to_lines_with_indicator<'a>(
    dl: &'a DisplayLine,
    effective_mode: ResultDisplay,
    theme: &Theme,
    content_width: u16,
) -> Vec<Line<'a>> {
    if let DisplayLine::ToolUse {
        tool,
        input_preview,
    } = dl
    {
        let color = theme.tool_color(tool);
        let marker = theme.marker_char;
        let chevron = match effective_mode {
            ResultDisplay::Full | ResultDisplay::Diff => CHEVRON_EXPANDED,
            ResultDisplay::Compact | ResultDisplay::Hidden => CHEVRON_COLLAPSED,
        };
        let mut spans = vec![
            Span::styled(
                format!("{marker} "),
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
        display_line_to_lines(dl, theme, content_width)
    }
}

/// Convert a [`DisplayLine`] to one or more styled ratatui [`Line`]s.
///
/// Handles all variants except [`DisplayLine::Text`] and
/// [`DisplayLine::ToolResult`], which are rendered inline (via
/// `markdown::render_markdown`) and by [`render_tool_result`] respectively with
/// run-position awareness.
fn display_line_to_lines<'a>(
    dl: &'a DisplayLine,
    theme: &Theme,
    content_width: u16,
) -> Vec<Line<'a>> {
    match dl {
        // Text is handled by render_text_line() for run tracking.
        DisplayLine::Text(_) => Vec::new(),

        DisplayLine::ToolUse {
            tool,
            input_preview,
        } => {
            let color = theme.tool_color(tool);
            let marker = theme.marker_char;
            let mut spans = vec![
                Span::styled(
                    format!("{marker} "),
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
            Span::styled("~ ", Style::default().fg(theme.thinking_color)),
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

        DisplayLine::Error(s) => vec![Line::from(vec![
            Span::styled(
                "! ",
                Style::default()
                    .fg(theme.error_text)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                s.as_str(),
                Style::default()
                    .fg(theme.error_text)
                    .add_modifier(Modifier::BOLD),
            ),
        ])],

        DisplayLine::TurnSummary {
            input_tokens,
            output_tokens,
            cost_usd,
        } => {
            let input_str = format_token_count(*input_tokens);
            let output_str = format_token_count(*output_tokens);
            let cost_str = format_cost(*cost_usd);
            let summary = format!("{input_str} in / {output_str} out | {cost_str}");
            vec![Line::from(Span::styled(
                summary,
                Style::default()
                    .fg(theme.turn_separator_meta)
                    .add_modifier(Modifier::DIM),
            ))]
        }

        DisplayLine::TurnStart { turn_number } => {
            let n = turn_number.unwrap_or(0);
            let label = format!(" Turn {} ", n);
            let rule_char = "─";
            let prefix = rule_char.repeat(4);
            let effective_width = (content_width as usize).saturating_sub(2).max(20);
            let suffix_len = effective_width.saturating_sub(4 + label.len());
            let suffix = rule_char.repeat(suffix_len);
            let dim_rule = Style::default()
                .fg(theme.turn_separator)
                .add_modifier(Modifier::DIM);
            vec![
                Line::from(""),
                Line::from(vec![
                    Span::styled(prefix, dim_rule),
                    Span::styled(
                        label,
                        Style::default()
                            .fg(theme.turn_separator_label)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(suffix, dim_rule),
                ]),
                Line::from(""),
            ]
        }

        // DiffResult is handled by render_diff_block() via CollapsedBlock::DiffBlock.
        DisplayLine::DiffResult { .. } => Vec::new(),

        DisplayLine::UserPrompt { content, queued } => {
            // User prompt marker: bold colored '>' prefix with regular-weight text.
            // Multiline prompts get continuation lines indented to align with the
            // first line's text (past the "> " prefix).
            //
            // When `queued` is true a right-aligned `[queued]` label is appended to
            // the first line in dim Yellow (status_message color) to visually
            // distinguish messages that haven't been processed yet.
            const USER_MARKER: &str = "> ";
            let marker_style = Style::default()
                .fg(theme.user_prompt_marker)
                .add_modifier(Modifier::BOLD);

            if content.is_empty() {
                let mut spans = vec![Span::styled(USER_MARKER.to_string(), marker_style)];
                if *queued {
                    let used = UnicodeWidthStr::width(USER_MARKER) + 8; // 8 for "[queued]"
                    let pad = (content_width as usize).saturating_sub(used);
                    spans.push(Span::raw(" ".repeat(pad)));
                    spans.push(Span::styled(
                        "[queued]",
                        Style::default()
                            .fg(theme.status_message)
                            .add_modifier(Modifier::DIM),
                    ));
                }
                return vec![Line::from(spans)];
            }

            let indent = " ".repeat(USER_MARKER.len());
            let mut result_lines = Vec::new();
            for (idx, text_line) in content.lines().enumerate() {
                if idx == 0 {
                    let mut spans = vec![
                        Span::styled(USER_MARKER.to_string(), marker_style),
                        Span::styled(
                            text_line.to_string(),
                            Style::default().fg(theme.user_prompt_fg),
                        ),
                    ];
                    if *queued {
                        // Right-align the [queued] label with padding.
                        let used = UnicodeWidthStr::width(USER_MARKER)
                            + UnicodeWidthStr::width(text_line)
                            + 9; // 8 for "[queued]" + 1 space
                        let pad = (content_width as usize).saturating_sub(used);
                        spans.push(Span::raw(" ".repeat(pad.max(1))));
                        spans.push(Span::styled(
                            "[queued]",
                            Style::default()
                                .fg(theme.status_message)
                                .add_modifier(Modifier::DIM),
                        ));
                    }
                    result_lines.push(Line::from(spans));
                } else {
                    result_lines.push(Line::from(vec![
                        Span::styled(indent.clone(), Style::default()),
                        Span::styled(
                            text_line.to_string(),
                            Style::default().fg(theme.user_prompt_fg),
                        ),
                    ]));
                }
            }
            result_lines
        }

        DisplayLine::AgentMessage {
            sender,
            recipient,
            content,
        } => {
            // Envelope marker with blue styling to visually distinguish
            // inter-agent messages from regular tool output.
            let arrow = format!("@{sender} -> @{recipient}");
            vec![Line::from(vec![
                Span::styled(
                    "@ ",
                    Style::default()
                        .fg(theme.result_marker)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    arrow,
                    Style::default()
                        .fg(theme.result_marker)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    format!(": {content}"),
                    Style::default()
                        .fg(theme.result_text)
                        .add_modifier(Modifier::ITALIC),
                ),
            ])]
        }
    }
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

/// Format a token count as a compact human-readable string.
///
/// - Under 1,000: `999`
/// - Under 1,000,000: `12.5K`
/// - 1,000,000 or more: `1.5M`
fn format_token_count(count: u64) -> String {
    if count >= 1_000_000 {
        format!("{:.1}M", count as f64 / 1_000_000.0)
    } else if count >= 1_000 {
        format!("{:.1}K", count as f64 / 1_000.0)
    } else {
        count.to_string()
    }
}

/// Format a cost in USD as a compact string.
///
/// - Under $100: two decimal places (`$0.15`, `$99.99`)
/// - $100 to $9999: integer only (`$123`, `$9999`)
/// - $10000+: K suffix (`$10.0K`, `$150K`)
fn format_cost(cost: f64) -> String {
    if cost >= 10_000.0 {
        let k = cost / 1_000.0;
        if k >= 100.0 {
            format!("${:.0}K", k)
        } else {
            format!("${:.1}K", k)
        }
    } else if cost >= 100.0 {
        format!("${:.0}", cost)
    } else {
        format!("${:.2}", cost)
    }
}

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

/// Compute the display-column position after each character when wrapping at
/// `width` columns.  This is the single source of truth for character-level
/// wrapping — [`count_wrapped_lines`], [`char_wrap`], and the cursor position
/// calculation in [`render_inline_input`] all delegate to this function so
/// they can never drift out of sync.
///
/// Calls `callback(ch, col_before, row)` for every character in `text`.
/// `col_before` is the column the character starts at (after any wrap),
/// and `row` is the zero-based visual row.
fn walk_wrapped<F: FnMut(char, usize, usize)>(text: &str, width: usize, mut callback: F) {
    let mut col = 0usize;
    let mut row = 0usize;
    for ch in text.chars() {
        if ch == '\n' {
            callback(ch, col, row);
            col = 0;
            row += 1;
        } else {
            let w = ch.width().unwrap_or(0);
            if width > 0 && col + w > width {
                col = 0;
                row += 1;
            }
            callback(ch, col, row);
            col += w;
        }
    }
}

/// Count how many visual lines `text` would occupy when wrapped at `width`
/// display columns.  Returns at least 1 (an empty string still occupies one
/// line).
fn count_wrapped_lines(text: &str, width: usize) -> usize {
    let mut max_row = 0usize;
    walk_wrapped(text, width, |_, _, row| {
        max_row = row;
    });
    max_row + 1
}

/// Wrap `text` at exactly `width` display columns, inserting newlines so that
/// no visual line exceeds `width` columns.  Explicit `\n` in the input starts
/// a new line as usual.  Uses `UnicodeWidthChar` so that CJK characters and
/// emoji (display width 2) are handled correctly.
fn char_wrap(text: &str, width: usize) -> String {
    if width == 0 {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len() + text.len() / width);
    let mut prev_row = 0usize;
    walk_wrapped(text, width, |ch, _, row| {
        if row > prev_row && ch != '\n' {
            out.push('\n');
        }
        prev_row = row;
        out.push(ch);
    });
    out
}

/// Compute the `(col, row)` cursor position after walking through `byte_len`
/// bytes of `text` with character-level wrapping at `width` columns.
fn cursor_pos_wrapped(text: &str, byte_len: usize, width: usize) -> (usize, usize) {
    let mut cx = 0usize;
    let mut cy = 0usize;
    let mut bytes_seen = 0usize;
    walk_wrapped(text, width, |ch, col, row| {
        if bytes_seen < byte_len {
            cx = col + ch.width().unwrap_or(0);
            cy = row;
            // For newlines, the cursor sits at column 0 on the next row.
            // If this newline is the last character before `byte_len`,
            // there won't be a subsequent iteration where `row` advances,
            // so we set `cy = row + 1` explicitly.
            if ch == '\n' {
                cx = 0;
                cy = row + 1;
            }
            bytes_seen += ch.len_utf8();
        }
    });
    (cx, cy)
}

/// Render the persistent inline chat input area.
///
/// When the area has zero height (e.g. on the welcome screen) this is a no-op.
/// Otherwise it draws:
/// - A top border separating it from the chat output above
/// - Placeholder text when the buffer is empty
/// - The chat input text with word wrapping when non-empty
/// - A blinking-style cursor when focused (`InputMode::Chat`)
/// - The focused agent name as a context indicator on the border
fn render_inline_input(app: &App, frame: &mut Frame, area: Rect) {
    if area.height == 0 {
        return;
    }
    let theme = &app.theme;
    let focused = app.input_mode == InputMode::Chat;

    let border_color = if focused {
        theme.chat_input_focused_border
    } else {
        theme.chat_input_border
    };

    let block = Block::default()
        .borders(Borders::TOP)
        .border_style(Style::default().fg(border_color));

    let inner = block.inner(area);
    frame.render_widget(block, area);

    // Retrieve the focused agent's status and queue length for placeholder logic.
    let focused_agent = app.agents.get(app.focused);
    let agent_status = focused_agent.map(|a| &a.status);
    let queue_len = focused_agent.map(|a| a.message_queue.len()).unwrap_or(0);

    let agent_pending = matches!(agent_status, Some(AgentStatus::Pending));
    let agent_running = matches!(agent_status, Some(AgentStatus::Running));

    let text = app.chat_input.text();

    if text.is_empty() && !focused {
        // Show placeholder with context-aware hint based on agent status.
        let placeholder_text = if agent_pending {
            "Type your first message to start this agent..."
        } else if agent_running {
            if queue_len >= super::state::MAX_MESSAGE_QUEUE {
                "Queue full -- wait for agent to finish before sending more messages"
            } else {
                "Press i to chat -- messages will queue until agent finishes..."
            }
        } else if matches!(agent_status, Some(AgentStatus::Exited(Some(n))) if *n != 0) {
            "Press i to retry or send a new message..."
        } else {
            // Exited(None), Exited(Some(0)), or no agent — success / default.
            "Press i to send a follow-up message..."
        };
        let placeholder = Paragraph::new(placeholder_text)
            .style(Style::default().fg(theme.chat_input_placeholder));
        frame.render_widget(placeholder, inner);
    } else if text.is_empty() && focused {
        // Focused but empty — show placeholder and cursor at start
        let placeholder = Paragraph::new("Type a message...")
            .style(Style::default().fg(theme.chat_input_placeholder));
        frame.render_widget(placeholder, inner);
        if inner.width > 0 && inner.height > 0 {
            frame.set_cursor_position((inner.x, inner.y));
        }
    } else {
        // Render the input text with character-level wrapping.
        //
        // We manually wrap the text into lines so that the cursor position
        // calculation matches exactly what is rendered. Ratatui's
        // `Wrap { trim: false }` breaks at word boundaries, which would
        // cause the cursor to drift on wrapped lines.
        let line_width = inner.width as usize;
        let wrapped = char_wrap(text, line_width);

        // Compute a vertical scroll offset so the cursor row is always
        // visible. When the wrapped text exceeds the available height we
        // scroll just enough to keep the cursor's row within the viewport.
        let visible_rows = inner.height as usize;
        let byte_pos = app.chat_input.cursor_pos().min(text.len());
        let (cx, cy) = cursor_pos_wrapped(text, byte_pos, line_width);
        let scroll_offset = if cy >= visible_rows {
            (cy - visible_rows + 1) as u16
        } else {
            0
        };

        let paragraph = Paragraph::new(wrapped.as_str())
            .style(
                Style::default()
                    .fg(theme.chat_input_fg)
                    .bg(theme.chat_input_bg),
            )
            .scroll((scroll_offset, 0));
        frame.render_widget(paragraph, inner);

        // Render cursor when focused
        if focused && inner.width > 0 && inner.height > 0 {
            let cursor_x = inner.x + cx as u16;
            let cursor_y = inner.y + (cy as u16).saturating_sub(scroll_offset);
            if cursor_y < inner.y + inner.height {
                frame.set_cursor_position((cursor_x, cursor_y));
            }
        }
    }

    // Context indicator: show focused agent name on the right side of the border,
    // with queue count when messages are queued.
    if let Some(agent) = app.agents.get(app.focused) {
        let queue_len = agent.message_queue.len();
        if queue_len > 0 {
            let is_full = queue_len >= super::state::MAX_MESSAGE_QUEUE;
            let queue_label = if is_full {
                format!("[{}/{} FULL] ", queue_len, super::state::MAX_MESSAGE_QUEUE)
            } else {
                format!("[{queue_len} queued] ")
            };
            let label = format!(" {} {}", agent.name, queue_label);
            let label_width = label.width() as u16;
            if area.width > label_width + 2 {
                let x = area.x + area.width - label_width - 1;
                // Render agent name in border color, queue count in
                // status_message color normally or error_text when full.
                let queue_style = if is_full {
                    Style::default()
                        .fg(theme.error_text)
                        .add_modifier(Modifier::BOLD)
                } else {
                    Style::default()
                        .fg(theme.status_message)
                        .add_modifier(Modifier::BOLD)
                };
                let spans = vec![
                    Span::styled(
                        format!(" {} ", agent.name),
                        Style::default().fg(border_color),
                    ),
                    Span::styled(queue_label, queue_style),
                ];
                frame.render_widget(Line::from(spans), Rect::new(x, area.y, label_width, 1));
            }
        } else {
            let label = format!(" {} ", agent.name);
            let label_width = label.width() as u16;
            if area.width > label_width + 2 {
                let x = area.x + area.width - label_width - 1;
                let span = Span::styled(label, Style::default().fg(border_color));
                frame.render_widget(span, Rect::new(x, area.y, label_width, 1));
            }
        }
    }
}

/// Build the left-side status spans for the status bar.
///
/// This pure function extracts the span-building logic so it can be unit-tested
/// without requiring a full ratatui terminal/frame setup.
///
/// Returns a `Vec<Span>` containing the activity indicator, elapsed time,
/// and turn/tool counts (abbreviated or full depending on width).
fn build_status_left_spans<'a>(
    width: u16,
    activity: &AgentActivity,
    status: &AgentStatus,
    turn_count: usize,
    tool_count: usize,
    elapsed_str: &str,
    theme: &'a Theme,
) -> Vec<Span<'a>> {
    let (activity_label, activity_color) = if matches!(status, AgentStatus::Pending) {
        ("Pending", theme.activity_idle)
    } else {
        match activity {
            AgentActivity::Idle => ("Idle", theme.activity_idle),
            AgentActivity::Thinking => ("Thinking", theme.activity_thinking),
            AgentActivity::Tool(_) => ("", theme.activity_tool),
            AgentActivity::Done => ("Done", theme.activity_done),
        }
    };

    let activity_span = if let AgentActivity::Tool(name) = activity {
        let max_tool_len = if width >= 60 { 20 } else { 12 };
        let display_name = truncate_chars(name, max_tool_len, "...");
        Span::styled(
            format!("{ARROW_RIGHT} {display_name}"),
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

    let turns_label = if turn_count == 1 { "turn" } else { "turns" };
    let tools_label = if tool_count == 1 { "tool" } else { "tools" };

    let mut spans = vec![Span::raw(" "), activity_span];
    if width >= 45 {
        spans.push(Span::raw("  "));
        spans.push(Span::raw(elapsed_str.to_string()));
    }
    if width >= 60 {
        spans.push(Span::raw("    "));
        spans.push(Span::raw(format!("{} {turns_label}", turn_count)));
        spans.push(Span::raw("  "));
        spans.push(Span::raw(format!("{} {tools_label}", tool_count)));
    } else if width >= 45 {
        spans.push(Span::raw("    "));
        spans.push(Span::raw(format!("{turn_count}t")));
        spans.push(Span::raw("  "));
        spans.push(Span::raw(format!("{tool_count}T")));
    }

    spans
}

/// Return a responsive display label for the agent's model setting.
///
/// Strips the `claude-` prefix for known models at wider widths, abbreviates to
/// the family name or a single uppercase letter at narrower widths, and hides
/// entirely below 45 columns. Non-standard model strings fall back to truncation.
/// Returns `None` when the input is `None`.
fn format_model_label(model: &Option<String>, width: u16) -> Option<String> {
    let value = model.as_deref()?;

    if width < 45 {
        return None;
    }

    // Known model families: (full value, stripped, family, initial)
    let known: &[(&str, &str, &str, &str)] = &[
        ("claude-opus-4-6", "opus-4-6", "opus", "O"),
        ("claude-sonnet-4-5", "sonnet-4-5", "sonnet", "S"),
        ("claude-haiku-4-5", "haiku-4-5", "haiku", "H"),
    ];

    for &(full, stripped, family, initial) in known {
        if value == full {
            return Some(if width >= 100 {
                stripped.to_string()
            } else if width >= 80 {
                family.to_string()
            } else {
                initial.to_string()
            });
        }
    }

    // Non-standard model: truncation fallback.
    Some(if width >= 100 {
        if value.len() <= 12 {
            value.to_string()
        } else {
            value.chars().take(12).collect()
        }
    } else if width >= 80 {
        if value.len() <= 8 {
            value.to_string()
        } else {
            value.chars().take(8).collect()
        }
    } else {
        // >= 45
        value
            .chars()
            .next()
            .map(|c| c.to_uppercase().to_string())
            .unwrap_or_default()
    })
}

/// Return a responsive display label for the agent's permission mode setting.
///
/// Known modes (`default`, `plan`, `acceptEdits`, `bypassPermissions`) are
/// shortened at the 80-column tier and reduced to a single lowercase letter at
/// 45 columns. Hidden below 45 columns. Returns `None` when the input is `None`.
fn format_permission_label(perm: &Option<String>, width: u16) -> Option<String> {
    let value = perm.as_deref()?;

    if width < 45 {
        return None;
    }

    // (full value, >= 100, >= 80, >= 45)
    let known: &[(&str, &str, &str, &str)] = &[
        ("default", "default", "default", "d"),
        ("plan", "plan", "plan", "p"),
        ("acceptEdits", "acceptEdits", "accept", "a"),
        ("bypassPermissions", "bypassPermissions", "bypass", "b"),
    ];

    for &(full, wide, medium, narrow) in known {
        if value == full {
            return Some(if width >= 100 {
                wide.to_string()
            } else if width >= 80 {
                medium.to_string()
            } else {
                narrow.to_string()
            });
        }
    }

    // Unknown permission value: return as-is (shouldn't happen with known constants).
    Some(value.to_string())
}

/// Return a responsive display label for the agent's effort level setting.
///
/// Full label at >= 100 columns, short form (`hi`/`med`/`lo`) at >= 80 and
/// >= 45 columns (same abbreviation for both tiers), hidden below 45 columns.
/// Returns `None` when the input is `None`.
fn format_effort_label(effort: &Option<String>, width: u16) -> Option<String> {
    let value = effort.as_deref()?;

    if width < 45 {
        return None;
    }

    // (full value, >= 100, >= 80 / >= 45)
    let known: &[(&str, &str, &str)] = &[
        ("high", "high", "hi"),
        ("medium", "medium", "med"),
        ("low", "low", "lo"),
    ];

    for &(full, wide, short) in known {
        if value == full {
            return Some(if width >= 100 {
                wide.to_string()
            } else {
                short.to_string()
            });
        }
    }

    // Unknown effort value: return as-is.
    Some(value.to_string())
}

/// Compose model, permission-mode, and effort-level indicators into styled spans.
///
/// Calls each `format_*_label` helper, collects non-`None` results, and joins
/// them with two-space separators. Returns an empty `Vec` when all options are
/// `None` or the terminal width is below 45 columns.
///
/// The caller is responsible for adding any pipe separator between these spans
/// and adjacent content (e.g., the token/cost cluster).
fn build_indicator_spans<'a>(
    options: &ClaudeOptions,
    width: u16,
    theme: &'a Theme,
) -> Vec<Span<'a>> {
    if width < 45 {
        return Vec::new();
    }

    let labels: Vec<String> = [
        format_model_label(&options.model, width),
        format_permission_label(&options.permission_mode, width),
        format_effort_label(&options.effort, width),
    ]
    .into_iter()
    .flatten()
    .collect();

    if labels.is_empty() {
        return Vec::new();
    }

    let style = Style::default().fg(theme.status_bar_fg);

    let mut spans = Vec::new();
    for (i, label) in labels.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled("  ", style));
        }
        spans.push(Span::styled(label.clone(), style));
    }

    spans
}

/// Render the two-line status bar at the bottom.
fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // Split the 2-line area into two 1-line rows.
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    // Pre-compute whether the focused agent is actively running (used for hint bar).
    let agent_active = app
        .focused_agent()
        .map(|a| matches!(a.activity, AgentActivity::Thinking | AgentActivity::Tool(_)))
        .unwrap_or(false);

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
                " No agent — press n to create one",
                Style::default().fg(theme.status_no_agent),
            )),
            Some(agent) => {
                let w = area.width;

                let elapsed = agent.active_elapsed();
                let elapsed_str = format_elapsed(elapsed);

                let mut left_spans = build_status_left_spans(
                    w,
                    &agent.activity,
                    &agent.status,
                    agent.turn_count,
                    agent.tool_count,
                    &elapsed_str,
                    theme,
                );

                // Queue indicator (only when non-empty)
                let queue_len = agent.message_queue.len();
                if queue_len > 0 {
                    left_spans.push(Span::raw("  "));
                    if queue_len >= super::state::MAX_MESSAGE_QUEUE {
                        left_spans.push(Span::styled(
                            format!("{}/{} FULL", queue_len, super::state::MAX_MESSAGE_QUEUE),
                            Style::default().fg(theme.error_text),
                        ));
                    } else {
                        left_spans.push(Span::raw(format!("{queue_len} queued")));
                    }
                }

                // Scroll indicator (unified bracketed format)
                if agent.scroll_offset > 0 {
                    left_spans.push(Span::raw("  "));
                    if agent.has_new_output {
                        left_spans.push(Span::styled(
                            "[new output]".to_string(),
                            Style::default()
                                .fg(theme.new_output)
                                .add_modifier(Modifier::BOLD),
                        ));
                    } else {
                        let line_pos = agent.output.len().saturating_sub(agent.scroll_offset);
                        left_spans.push(Span::styled(
                            format!("[line {}]", line_pos),
                            Style::default().fg(theme.status_bar_fg),
                        ));
                    }
                }

                // Right side: indicator spans + tokens + cost
                // Width-aware: drop cost at <80 cols, drop entire right side at <45 cols
                let indicator_spans =
                    build_indicator_spans(&agent.claude_options, w, theme);

                let mut token_cost_spans: Vec<Span> = Vec::new();
                if w >= 45
                    && (agent.input_tokens != 0
                        || agent.output_tokens != 0
                        || agent.total_cost_usd != 0.0)
                {
                    let input = format_token_count(agent.input_tokens);
                    let output = format_token_count(agent.output_tokens);
                    if w >= 80 {
                        let cost = format_cost(agent.total_cost_usd);
                        token_cost_spans
                            .push(Span::raw(format!("{input} in  {output} out  {cost} ",)));
                    } else {
                        token_cost_spans
                            .push(Span::raw(format!("{input} in  {output} out ",)));
                    }
                }

                let mut right_spans: Vec<Span> = Vec::new();
                let has_indicators = !indicator_spans.is_empty();
                let has_token_cost = !token_cost_spans.is_empty();
                right_spans.extend(indicator_spans);
                if has_indicators && has_token_cost && w >= 80 {
                    right_spans.push(Span::styled(
                        " | ",
                        Style::default().fg(theme.status_bar_fg),
                    ));
                }
                right_spans.extend(token_cost_spans);

                // Build the full line with left fill + right
                let left_line = Line::from(left_spans);
                let right_line = Line::from(right_spans);

                // Use ratatui Layout to split row into left (fill) and right (length)
                let right_width = right_line.width() as u16;
                let row_chunks =
                    Layout::horizontal([Constraint::Fill(1), Constraint::Length(right_width)])
                        .split(rows[0]);

                let left_para = Paragraph::new(left_line).style(
                    Style::default()
                        .bg(theme.status_bar_bg)
                        .fg(theme.status_bar_fg),
                );
                let right_para = Paragraph::new(right_line)
                    .alignment(Alignment::Right)
                    .style(
                        Style::default()
                            .bg(theme.status_bar_bg)
                            .fg(theme.status_bar_fg),
                    );
                frame.render_widget(left_para, row_chunks[0]);
                frame.render_widget(right_para, row_chunks[1]);

                // The agent branch renders the status bar as a two-part
                // (left + right) layout that requires direct `frame.render_widget`
                // calls, so we render the hint bar here and return early to skip
                // the single-`Paragraph` fallback path below.
                let hints = build_hint_bar(app.input_mode, agent_active, area.width);
                let hints_paragraph = Paragraph::new(hints)
                    .style(Style::default().bg(theme.hint_bar_bg).fg(theme.hint_bar_fg));
                frame.render_widget(hints_paragraph, rows[1]);

                // Return early: the agent branch uses a two-part (left + right)
                // layout rendered directly via `frame.render_widget`, which is
                // incompatible with the single-`Paragraph` fallback below. Any
                // shared post-render logic must be duplicated above this point.
                return;
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
    let hints = build_hint_bar(app.input_mode, agent_active, area.width);

    let hints_paragraph =
        Paragraph::new(hints).style(Style::default().bg(theme.hint_bar_bg).fg(theme.hint_bar_fg));
    frame.render_widget(hints_paragraph, rows[1]);
}

/// Build the hint bar line with context-sensitive hotkeys based on the current
/// `InputMode` and agent state.
///
/// `agent_active` should be `true` when the focused agent is in `Thinking` or
/// `Tool` activity, which makes `Esc:interrupt` relevant in Normal mode.
fn build_hint_bar(mode: InputMode, agent_active: bool, width: u16) -> Line<'static> {
    let hints: &[(&str, &str)] = match mode {
        InputMode::Normal if agent_active => &[
            ("Esc", "interrupt"),
            ("i", "chat"),
            ("n", "new"),
            (":", "command"),
            ("/", "search"),
            ("Tab", "next"),
            ("?", "help"),
        ],
        InputMode::Normal => &[
            ("i", "chat"),
            ("n", "new"),
            ("o", "sessions"),
            (":", "command"),
            ("Tab", "next"),
            ("q", "close"),
            ("?", "help"),
        ],
        InputMode::Chat => &[
            ("Esc", "cancel"),
            ("Enter", "send"),
            ("Shift+Enter", "newline"),
            ("Up/Down", "history"),
            ("Ctrl+R", "search"),
        ],
        InputMode::Input => &[
            ("Esc", "cancel"),
            ("Tab", "next field"),
            ("Ctrl+S", "submit"),
            ("Ctrl+E", "toggle mode"),
            ("Ctrl+R", "history"),
        ],
        InputMode::Settings => &[
            ("Esc", "cancel"),
            ("Tab", "next field"),
            ("Enter", "save"),
            ("Left/Right", "cycle option"),
        ],
        InputMode::HistorySearch => &[
            ("Esc", "cancel"),
            ("Enter", "select"),
            ("Up/Down", "navigate"),
        ],
        InputMode::TemplatePicker => &[
            ("Esc", "cancel"),
            ("Enter", "select"),
            ("Up/Down", "navigate"),
            ("d", "delete"),
        ],
        InputMode::SaveTemplate => &[("Esc", "cancel"), ("Enter", "save")],
        InputMode::SessionPicker => &[
            ("Esc", "cancel"),
            ("Enter", "open"),
            ("Up/Down", "navigate"),
        ],
        InputMode::Search => &[
            ("Esc", "cancel"),
            ("Enter", "find next"),
            ("n/N", "next/prev"),
            (":", "command"),
        ],
        InputMode::Command => &[
            ("Esc", "cancel"),
            ("Enter", "execute"),
            ("Up/Down", "navigate"),
            ("Tab", "complete"),
        ],
    };

    // Truncate hints to fit within available width.
    // Each hint occupies: key_len + 1 (colon) + desc_len [+ 2 (separator) if not last].
    // Total includes 1 leading space.
    // Strategy: preserve first and last hints, remove from second-to-last backwards.
    let hint_width = |h: &(&str, &str)| -> usize { h.0.len() + 1 + h.1.len() };
    let available = width as usize;

    let mut visible: Vec<(&str, &str)> = hints.to_vec();

    // Calculate total width: 1 leading space + sum of hint widths + 2 separator per non-last hint
    let total_width = |v: &[(&str, &str)]| -> usize {
        if v.is_empty() {
            return 0;
        }
        1 + v.iter().map(hint_width).sum::<usize>() + 2 * v.len().saturating_sub(1)
    };

    // Remove hints from second-to-last working backwards until it fits
    while visible.len() > 2 && total_width(&visible) > available {
        visible.remove(visible.len() - 2);
    }
    // If still too wide with only 2 hints, try removing the last one
    if visible.len() == 2 && total_width(&visible) > available {
        visible.pop();
    }

    let mut spans = Vec::new();
    spans.push(Span::raw(" "));
    for (i, (key, desc)) in visible.iter().enumerate() {
        spans.push(Span::styled(
            key.to_string(),
            Style::default().add_modifier(Modifier::BOLD),
        ));
        if i + 1 < visible.len() {
            spans.push(Span::raw(format!(":{desc}  ")));
        } else {
            spans.push(Span::raw(format!(":{desc}")));
        }
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

/// Render a centered settings overlay for editing the focused agent's Claude
/// options (permission mode, model, effort, max budget, allowed tools, add-dir).
///
/// Layout: Core Configuration (Model, Permission Mode, Effort) with three-line
/// selector fields (UPPERCASE label, description, selector), then an Advanced
/// section divider, then text fields (Workspace, Budget, Tools, Dirs) with
/// inline validation errors.
fn render_settings(app: &mut App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;

    // ── Width clamping ──
    // Desired = 60% of terminal, clamped between 50 and 80, never wider than terminal.
    let desired_width = area.width * 60 / 100;
    let popup_width = desired_width.max(50).min(80).min(area.width);

    // Narrow mode: hide descriptions and use stacked text field layout.
    // Based on terminal width, not popup width (popup is always ≤80).
    let narrow = area.width < 60;

    // Description style: dim gray (only used in wide mode).
    let desc_style = Style::default()
        .fg(theme.option_unset)
        .add_modifier(Modifier::DIM);

    // Build field lines using the same helpers as render_input.
    let model_lines = build_selector(
        MODELS,
        app.model.text(),
        app.input_field == InputField::Model,
        theme,
    );
    let perm_lines = build_selector(
        PERMISSION_MODES,
        app.permission_mode.text(),
        app.input_field == InputField::PermissionMode,
        theme,
    );
    let effort_lines = build_selector(
        EFFORT_LEVELS,
        app.effort.text(),
        app.input_field == InputField::Effort,
        theme,
    );

    // Helper: check if settings_error targets a specific field.
    let error_for = |field: InputField| -> Option<&str> {
        app.settings_error
            .as_ref()
            .filter(|(f, _)| *f == field)
            .map(|(_, msg)| msg.as_str())
    };

    // Column 1 width for compact two-column text fields.
    const LABEL_COL_WIDTH: usize = 18;

    let mut text = Vec::new();

    // Track the starting line index for each field so we can auto-scroll.
    let mut focused_field_line: Option<usize> = None;
    let mut focused_field_end: Option<usize> = None;

    // ── Core Configuration: Selector fields (3-line or 2-line format) ──

    // ── Model ──
    if app.input_field == InputField::Model {
        focused_field_line = Some(text.len());
    }
    text.push(Line::from(Span::styled(
        "MODEL",
        field_label_style(app.input_field == InputField::Model, theme),
    )));
    if !narrow {
        text.push(Line::from(Span::styled(
            "Which Claude model to use for this agent",
            desc_style,
        )));
    }
    text.extend(model_lines);
    if app.input_field == InputField::Model {
        focused_field_end = Some(text.len());
    }

    text.push(Line::from("")); // blank separator

    // ── Permission Mode ──
    if app.input_field == InputField::PermissionMode {
        focused_field_line = Some(text.len());
    }
    text.push(Line::from(Span::styled(
        "PERMISSION MODE",
        field_label_style(app.input_field == InputField::PermissionMode, theme),
    )));
    if !narrow {
        text.push(Line::from(Span::styled(
            "How Claude handles file edits and tool permissions",
            desc_style,
        )));
    }
    text.extend(perm_lines);
    if app.input_field == InputField::PermissionMode {
        focused_field_end = Some(text.len());
    }

    text.push(Line::from("")); // blank separator

    // ── Effort ──
    if app.input_field == InputField::Effort {
        focused_field_line = Some(text.len());
    }
    text.push(Line::from(Span::styled(
        "EFFORT",
        field_label_style(app.input_field == InputField::Effort, theme),
    )));
    if !narrow {
        text.push(Line::from(Span::styled(
            "Reasoning effort level (affects speed and token usage)",
            desc_style,
        )));
    }
    text.extend(effort_lines);
    if app.input_field == InputField::Effort {
        focused_field_end = Some(text.len());
    }

    text.push(Line::from("")); // blank separator

    // ── Advanced section divider ──
    text.push(Line::from(Span::styled(
        "-- Advanced -------------------------------------------------------",
        Style::default()
            .fg(theme.options_header)
            .add_modifier(Modifier::DIM),
    )));

    text.push(Line::from("")); // blank separator after divider

    // ── Advanced Configuration: Compact two-column text fields ──
    // In wide mode: single line with fixed-width label + value.
    // In narrow mode: stacked layout (label on one line, value on next).

    // Helper to build a compact (or stacked) field line.
    // Inner width available for content (popup minus left/right border).
    let inner_width = popup_width.saturating_sub(2) as usize;
    let build_compact_line = |label: &str,
                              buffer: &str,
                              cursor: usize,
                              active: bool,
                              optional: bool,
                              stacked: bool|
     -> Vec<Line<'static>> {
        // Max chars available for the value portion.
        let max_val = if stacked {
            inner_width
        } else {
            inner_width.saturating_sub(LABEL_COL_WIDTH)
        };

        // Truncate long values from the left, showing "…{tail}".
        let truncated: std::borrow::Cow<'_, str> =
            if !active && buffer.len() > max_val && max_val > 1 {
                let tail_start = buffer.len() - (max_val - 1);
                std::borrow::Cow::Owned(format!("…{}", &buffer[tail_start..]))
            } else {
                std::borrow::Cow::Borrowed(buffer)
            };

        let value_spans = if optional && buffer.is_empty() && !active {
            vec![Span::styled(
                "(unset)".to_string(),
                Style::default()
                    .fg(theme.option_unset)
                    .add_modifier(Modifier::DIM),
            )]
        } else if active {
            // When editing, show a scrolling window around the cursor.
            let buf = truncated.as_ref();
            let cur = cursor.min(buf.len());
            // If the full text fits, show it all; otherwise scroll to keep cursor visible.
            let (display, display_cur) = if buf.len() <= max_val {
                (buf.to_string(), cur)
            } else {
                // Center a window of max_val chars around the cursor.
                let half = max_val / 2;
                let start = if cur <= half {
                    0
                } else if cur + half >= buf.len() {
                    buf.len().saturating_sub(max_val)
                } else {
                    cur - half
                };
                let end = (start + max_val).min(buf.len());
                (buf[start..end].to_string(), cur - start)
            };
            let before = &display[..display_cur.min(display.len())];
            let after = &display[display_cur.min(display.len())..];
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
        } else {
            vec![Span::raw(truncated.into_owned())]
        };

        if stacked {
            // Stacked: label on its own line, value on the next.
            let label_line = Line::from(Span::styled(
                label.to_string(),
                field_label_style(active, theme),
            ));
            let value_line = Line::from(value_spans);
            vec![label_line, value_line]
        } else {
            // Two-column: fixed-width label + value on one line.
            let label_span = Span::styled(
                format!("{:<width$}", label, width = LABEL_COL_WIDTH),
                field_label_style(active, theme),
            );
            let mut spans = vec![label_span];
            spans.extend(value_spans);
            vec![Line::from(spans)]
        }
    };

    // ── Workspace ──
    if app.input_field == InputField::Workspace {
        focused_field_line = Some(text.len());
    }
    text.extend(build_compact_line(
        "Workspace",
        app.workspace.text(),
        app.workspace.cursor_pos(),
        app.input_field == InputField::Workspace,
        false,
        narrow,
    ));
    if let Some(msg) = error_for(InputField::Workspace) {
        text.push(Line::from(Span::styled(
            format!(
                "{:width$}^ {msg}",
                "",
                width = if narrow { 0 } else { LABEL_COL_WIDTH }
            ),
            Style::default().fg(theme.error_text),
        )));
    }
    if app.input_field == InputField::Workspace {
        focused_field_end = Some(text.len());
    }

    // ── Max Budget USD ──
    if app.input_field == InputField::MaxBudgetUsd {
        focused_field_line = Some(text.len());
    }
    text.extend(build_compact_line(
        "Max Budget USD",
        app.max_budget.text(),
        app.max_budget.cursor_pos(),
        app.input_field == InputField::MaxBudgetUsd,
        true,
        narrow,
    ));
    if let Some(msg) = error_for(InputField::MaxBudgetUsd) {
        text.push(Line::from(Span::styled(
            format!(
                "{:width$}^ {msg}",
                "",
                width = if narrow { 0 } else { LABEL_COL_WIDTH }
            ),
            Style::default().fg(theme.error_text),
        )));
    }
    if app.input_field == InputField::MaxBudgetUsd {
        focused_field_end = Some(text.len());
    }

    // ── Allowed Tools ──
    if app.input_field == InputField::AllowedTools {
        focused_field_line = Some(text.len());
    }
    text.extend(build_compact_line(
        "Allowed Tools",
        app.allowed_tools.text(),
        app.allowed_tools.cursor_pos(),
        app.input_field == InputField::AllowedTools,
        true,
        narrow,
    ));
    if let Some(msg) = error_for(InputField::AllowedTools) {
        text.push(Line::from(Span::styled(
            format!(
                "{:width$}^ {msg}",
                "",
                width = if narrow { 0 } else { LABEL_COL_WIDTH }
            ),
            Style::default().fg(theme.error_text),
        )));
    }
    if app.input_field == InputField::AllowedTools {
        focused_field_end = Some(text.len());
    }

    // ── Add Dir ──
    if app.input_field == InputField::AddDir {
        focused_field_line = Some(text.len());
    }
    text.extend(build_compact_line(
        "Add Dir",
        app.add_dir.text(),
        app.add_dir.cursor_pos(),
        app.input_field == InputField::AddDir,
        true,
        narrow,
    ));
    if let Some(msg) = error_for(InputField::AddDir) {
        text.push(Line::from(Span::styled(
            format!(
                "{:width$}^ {msg}",
                "",
                width = if narrow { 0 } else { LABEL_COL_WIDTH }
            ),
            Style::default().fg(theme.error_text),
        )));
    }
    if app.input_field == InputField::AddDir {
        focused_field_end = Some(text.len());
    }

    // ── Footer ──
    text.push(Line::from(""));
    text.push(
        Line::from(Span::styled(
            "Enter: save  |  Tab: next  |  Shift+Tab: prev  |  Ctrl+X: clear  |  Esc: cancel",
            Style::default().fg(theme.help_footer),
        ))
        .alignment(Alignment::Center),
    );

    // ── Popup dimensions ──
    let content_rows = text.len();
    // Cap height at 80% of terminal height.
    let max_height = (area.height as usize * 80 / 100).max(3);
    // +2 for top/bottom border.
    let popup_height = (content_rows + 2).min(max_height).min(area.height as usize) as u16;
    // Inner height available for content (subtract 2 for borders).
    let inner_height = popup_height.saturating_sub(2) as usize;

    // ── Auto-scroll to keep focused field visible ──
    if content_rows > inner_height {
        if let (Some(field_start), Some(field_end)) = (focused_field_line, focused_field_end) {
            // If the focused field starts before the visible window, scroll up.
            if field_start < app.settings_scroll {
                app.settings_scroll = field_start;
            }
            // If the focused field ends after the visible window, scroll down.
            if field_end > app.settings_scroll + inner_height {
                app.settings_scroll = field_end.saturating_sub(inner_height);
            }
        }
        // Clamp scroll to valid range.
        let max_scroll = content_rows.saturating_sub(inner_height);
        if app.settings_scroll > max_scroll {
            app.settings_scroll = max_scroll;
        }
    } else {
        app.settings_scroll = 0;
    }

    // Apply scroll offset: take the visible slice of content lines.
    let visible_text: Vec<Line> = text
        .into_iter()
        .skip(app.settings_scroll)
        .take(inner_height)
        .collect();

    let x = area.x + (area.width.saturating_sub(popup_width)) / 2;
    let y = area.y + area.height.saturating_sub(popup_height) / 2;
    let popup = Rect::new(x, y, popup_width, popup_height);
    frame.render_widget(Clear, popup);

    let title = if let Some(agent) = app.agents.get(app.focused) {
        format!(" Settings: {} ", agent.name)
    } else {
        " Agent Settings ".to_string()
    };

    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(theme.input_border));
    let inner = block.inner(popup);
    frame.render_widget(block, popup);

    let paragraph = Paragraph::new(visible_text).wrap(Wrap { trim: false });
    frame.render_widget(paragraph, inner);
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
            .unwrap_or_else(|| "?".to_string());
        format!(" Edit Agent {} ", label)
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
// Session picker overlay
// ---------------------------------------------------------------------------

/// Render the session picker overlay showing discovered Claude Code sessions.
fn render_session_picker(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let filtered = app.filtered_sessions();
    let total = filtered.len();

    let content_rows = total.max(1) + 3; // items + blank + footer + filter
    let popup_height = (content_rows + 2).min(area.height as usize).min(20) as u16;

    let popup = centered_rect_fixed_height(60, popup_height, area);
    frame.render_widget(Clear, popup);

    let mut text = Vec::new();

    // Filter indicator.
    let filter_text = app.session_filter.text();
    if !filter_text.is_empty() {
        text.push(Line::from(Span::styled(
            format!("  Filter: {filter_text}"),
            Style::default()
                .fg(theme.help_footer)
                .add_modifier(Modifier::ITALIC),
        )));
    }

    if total == 0 {
        text.push(Line::from(Span::styled(
            if filter_text.is_empty() {
                "  No sessions found for this workspace"
            } else {
                "  No matching sessions"
            },
            Style::default().fg(theme.help_footer),
        )));
    } else {
        for (i, session) in filtered.iter().enumerate() {
            let is_selected = app.session_selected == i;
            let slug_display = session
                .slug
                .as_deref()
                .unwrap_or(&session.session_id[..8.min(session.session_id.len())]);
            let role_indicator = match session.last_message_role.as_str() {
                "user" => ">",
                "assistant" => "<",
                _ => " ",
            };
            let preview = if session.last_message_preview.len() > 50 {
                let mut end = 47;
                while !session.last_message_preview.is_char_boundary(end) {
                    end -= 1;
                }
                format!("{}...", &session.last_message_preview[..end])
            } else {
                session.last_message_preview.clone()
            };
            let label = format!("  {role_indicator} {slug_display}  {preview}");

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
    }

    // Footer.
    text.push(Line::from(""));
    text.push(
        Line::from(Span::styled(
            "Up/Down: navigate  |  Enter: open  |  Type to filter  |  Esc: cancel",
            Style::default().fg(theme.help_footer),
        ))
        .alignment(Alignment::Center),
    );

    let picker = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.input_border))
                .title(" Previous Sessions ")
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
// Dependency graph overlay
// ---------------------------------------------------------------------------

/// Render a visual dependency graph showing agent nodes and communication edges.
///
/// Agents are displayed as labeled nodes arranged in a column. Directed edges
/// between agents represent inter-agent messages detected from
/// `DisplayLine::AgentMessage` events. Edge labels show the message count.
///
/// The graph is computed on each render from the agents' output buffers,
/// ensuring it always reflects the current state.
fn render_graph(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let popup = centered_rect(70, 80, area);

    // Clear the area behind the popup.
    frame.render_widget(Clear, popup);

    // Collect edges by scanning all agents' output for AgentMessage lines.
    let mut edge_counts: std::collections::HashMap<(usize, usize), usize> =
        std::collections::HashMap::new();

    for (agent_idx, agent) in app.agents.iter().enumerate() {
        for line in &agent.output {
            if let DisplayLine::AgentMessage {
                sender, recipient, ..
            } = line
            {
                // Resolve sender and recipient to agent indices.
                let sender_idx = app
                    .agents
                    .iter()
                    .position(|a| state::agent_name_matches(&a.name, sender));
                let recipient_idx = app
                    .agents
                    .iter()
                    .position(|a| state::agent_name_matches(&a.name, recipient));

                if let (Some(s), Some(r)) = (sender_idx, recipient_idx) {
                    // Only count each message once: count from the sender's
                    // perspective (avoid double-counting from recipient's copy).
                    if agent_idx == s && s != r {
                        *edge_counts.entry((s, r)).or_insert(0) += 1;
                    }
                }
            }
        }
    }

    let mut lines: Vec<Line<'_>> = Vec::new();

    let heading_style = Style::default()
        .fg(theme.help_heading)
        .add_modifier(Modifier::BOLD);

    lines.push(Line::from(Span::styled(
        "Agent Dependency Graph",
        heading_style,
    )));
    lines.push(Line::from(""));

    if app.agents.is_empty() {
        lines.push(Line::from(Span::styled(
            "No agents running",
            Style::default().fg(theme.content_empty),
        )));
    } else {
        // Render agent nodes.
        lines.push(Line::from(Span::styled("Nodes", heading_style)));
        lines.push(Line::from(""));

        for (idx, agent) in app.agents.iter().enumerate() {
            let label = workspace_label(&agent.workspace);
            let num = idx + 1;

            let (status_icon, icon_color) = match (&agent.status, &agent.activity) {
                (&AgentStatus::Pending, _) => ("\u{25CB}", theme.tab_badge_idle), // ○
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

            // Count connections for this agent.
            let outgoing: usize = edge_counts
                .iter()
                .filter(|((s, _), _)| *s == idx)
                .map(|(_, c)| c)
                .sum();
            let incoming: usize = edge_counts
                .iter()
                .filter(|((_, r), _)| *r == idx)
                .map(|(_, c)| c)
                .sum();

            let conn_info = if outgoing > 0 || incoming > 0 {
                format!("  [{ARROW_RIGHT}{outgoing} \u{25C0}{incoming}]")
            } else {
                String::new()
            };

            lines.push(Line::from(vec![
                Span::styled(format!("  {status_icon} "), Style::default().fg(icon_color)),
                Span::styled(
                    format!("[{num}] {label}"),
                    Style::default()
                        .fg(theme.tab_text)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(conn_info, Style::default().fg(theme.result_marker)),
            ]));
        }

        // Render edges.
        if !edge_counts.is_empty() {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled("Edges", heading_style)));
            lines.push(Line::from(""));

            // Sort edges by sender index for stable display.
            let mut sorted_edges: Vec<_> = edge_counts.iter().collect();
            sorted_edges.sort_by_key(|((s, r), _)| (*s, *r));

            for ((s_idx, r_idx), count) in sorted_edges {
                let s_label = workspace_label(&app.agents[*s_idx].workspace);
                let r_label = workspace_label(&app.agents[*r_idx].workspace);
                let s_num = s_idx + 1;
                let r_num = r_idx + 1;

                let count_label = if *count == 1 {
                    "1 message".to_string()
                } else {
                    format!("{count} messages")
                };

                lines.push(Line::from(vec![
                    Span::styled("  ", Style::default()),
                    Span::styled(
                        format!("[{s_num}] {s_label}"),
                        Style::default().fg(theme.result_marker),
                    ),
                    Span::styled(
                        " \u{2500}\u{2500}\u{25B6} ",
                        Style::default()
                            .fg(theme.tab_badge_spinner)
                            .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        format!("[{r_num}] {r_label}"),
                        Style::default().fg(theme.result_marker),
                    ),
                    Span::styled(
                        format!("  ({count_label})"),
                        Style::default().fg(theme.tool_result_hidden),
                    ),
                ]));
            }
        } else {
            lines.push(Line::from(""));
            lines.push(Line::from(Span::styled(
                "No inter-agent communication detected yet.",
                Style::default().fg(theme.content_empty),
            )));
            lines.push(Line::from(Span::styled(
                "Edges appear when agents send messages to each other.",
                Style::default().fg(theme.content_empty),
            )));
        }
    }

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        "Press 'v' to close",
        Style::default().fg(theme.help_footer),
    )));

    let graph = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.help_border))
                .title(" Dependency Graph ")
                .title_style(
                    Style::default()
                        .fg(theme.help_title)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.help_bg).fg(theme.help_fg));

    frame.render_widget(graph, popup);
}

// ---------------------------------------------------------------------------
// Dashboard overlay
// ---------------------------------------------------------------------------

/// Render the aggregate dashboard overlay showing all agent stats in a table.
///
/// Displays a table with one row per agent (status, activity, turns, tools,
/// elapsed time) plus aggregate totals at the bottom. The currently selected
/// row is highlighted and pressing Enter focuses that agent.
fn render_dashboard(app: &App, frame: &mut Frame, area: Rect) {
    let theme = &app.theme;
    let popup = centered_rect(90, 80, area);

    // Clear the area behind the popup.
    frame.render_widget(Clear, popup);

    // Show the PROMPT column only when the popup is wide enough.
    // Column width breakdown: #:4 + icon:3 + WORKSPACE:17 + ACTIVITY:13 + TURNS:7 + TOOLS:7 + ELAPSED:9 + borders:4 = 64
    let prompt_width = (popup.width as usize).saturating_sub(64).clamp(10, 40);
    let show_prompt = popup.width >= 74;

    let heading_style = Style::default()
        .fg(theme.help_heading)
        .add_modifier(Modifier::BOLD);
    let header_style = Style::default()
        .fg(theme.help_key)
        .add_modifier(Modifier::BOLD);
    let footer_style = Style::default().fg(theme.help_footer);
    let dim_style = Style::default().fg(theme.dashboard_dim);

    let mut lines: Vec<Line<'_>> = Vec::new();

    lines.push(Line::from(Span::styled("Agent Dashboard", heading_style)));
    lines.push(Line::from(""));

    if app.agents.is_empty() {
        lines.push(Line::from(Span::styled(
            "No agents running. Press 'n' to create one.",
            Style::default().fg(theme.content_empty),
        )));
    } else {
        // Table header.
        let pw = prompt_width;
        let header_text = if show_prompt {
            format!(
                " {:<3} {:<2} {:<16} {:<pw$} {:<12} {:>6} {:>6} {:>8} ",
                "#", "", "WORKSPACE", "PROMPT", "ACTIVITY", "TURNS", "TOOLS", "ELAPSED"
            )
        } else {
            format!(
                " {:<3} {:<2} {:<16} {:<12} {:>6} {:>6} {:>8} ",
                "#", "", "WORKSPACE", "ACTIVITY", "TURNS", "TOOLS", "ELAPSED"
            )
        };
        lines.push(Line::from(vec![Span::styled(header_text, header_style)]));
        lines.push(Line::from(Span::styled(
            format!(
                " {}",
                "\u{2500}".repeat(popup.width.saturating_sub(4) as usize)
            ),
            dim_style,
        )));

        // Aggregate accumulators.
        let mut total_turns: usize = 0;
        let mut total_tools: usize = 0;
        let mut running_count: usize = 0;
        let mut done_count: usize = 0;
        let mut error_count: usize = 0;

        for (idx, agent) in app.agents.iter().enumerate() {
            let is_selected = idx == app.dashboard_selected;

            let num = idx + 1;

            let (status_icon, icon_color) = match (&agent.status, &agent.activity) {
                (&AgentStatus::Pending, _) => ("\u{25CB}", theme.tab_badge_idle), // ○
                (AgentStatus::Exited(Some(0)), _) => {
                    done_count += 1;
                    (CHECK_MARK, theme.tab_badge_success)
                }
                (AgentStatus::Exited(_), _) => {
                    error_count += 1;
                    (CROSS_MARK, theme.tab_badge_error)
                }
                (AgentStatus::Running, AgentActivity::Idle) => {
                    running_count += 1;
                    (CIRCLE, theme.tab_badge_idle)
                }
                (AgentStatus::Running, AgentActivity::Done) => {
                    done_count += 1;
                    (CHECK_MARK, theme.tab_badge_success)
                }
                (AgentStatus::Running, _) => {
                    running_count += 1;
                    let frame_idx = app.tick % SPINNER_FRAMES.len();
                    (SPINNER_FRAMES[frame_idx], theme.tab_badge_spinner)
                }
            };

            let activity_label = match &agent.activity {
                AgentActivity::Idle => "idle".to_string(),
                AgentActivity::Thinking => "thinking".to_string(),
                AgentActivity::Tool(name) => truncate_chars(name, 10, "..."),
                AgentActivity::Done => "done".to_string(),
            };

            let label = workspace_label(&agent.workspace);
            let display_label = truncate_chars(&label, 16, "\u{2026}");

            let elapsed = format_elapsed(agent.active_elapsed());

            total_turns += agent.turn_count;
            total_tools += agent.tool_count;

            let row_text = format!(" {:<3} ", num,);
            let rest_text = if show_prompt {
                let prompt_preview = agent.prompt.lines().next().unwrap_or("");
                let display_prompt =
                    truncate_chars(prompt_preview, prompt_width.saturating_sub(2), "\u{2026}");
                format!(
                    " {:<16} {:<pw$} {:<12} {:>6} {:>6} {:>8} ",
                    display_label,
                    display_prompt,
                    activity_label,
                    agent.turn_count,
                    agent.tool_count,
                    elapsed,
                )
            } else {
                format!(
                    " {:<16} {:<12} {:>6} {:>6} {:>8} ",
                    display_label, activity_label, agent.turn_count, agent.tool_count, elapsed,
                )
            };

            let row_style = if is_selected {
                Style::default()
                    .bg(theme.dashboard_selected_bg)
                    .fg(theme.dashboard_selected_fg)
            } else {
                Style::default().fg(theme.help_fg)
            };

            lines.push(Line::from(vec![
                Span::styled(row_text, row_style),
                Span::styled(status_icon.to_string(), Style::default().fg(icon_color)),
                Span::styled(rest_text, row_style),
            ]));
        }

        // Separator.
        lines.push(Line::from(Span::styled(
            format!(
                " {}",
                "\u{2500}".repeat(popup.width.saturating_sub(4) as usize)
            ),
            dim_style,
        )));

        // Aggregate totals row.
        let totals_text = if show_prompt {
            format!(
                " {:<3} {:<2} {:<16} {:<pw$} {:<12} {:>6} {:>6} {:>8} ",
                "\u{03A3}", // Sigma
                "",
                format!("{}R {}D {}E", running_count, done_count, error_count),
                "",
                "",
                total_turns,
                total_tools,
                "",
            )
        } else {
            format!(
                " {:<3} {:<2} {:<16} {:<12} {:>6} {:>6} {:>8} ",
                "\u{03A3}", // Sigma
                "",
                format!("{}R {}D {}E", running_count, done_count, error_count),
                "",
                total_turns,
                total_tools,
                "",
            )
        };
        lines.push(Line::from(vec![Span::styled(
            totals_text,
            Style::default()
                .fg(theme.help_heading)
                .add_modifier(Modifier::BOLD),
        )]));
    }

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::styled("j/k", footer_style.add_modifier(Modifier::BOLD)),
        Span::styled(":navigate  ", footer_style),
        Span::styled("Enter", footer_style.add_modifier(Modifier::BOLD)),
        Span::styled(":focus  ", footer_style),
        Span::styled("D/Esc", footer_style.add_modifier(Modifier::BOLD)),
        Span::styled(":close", footer_style),
    ]));

    let dashboard = Paragraph::new(lines)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(theme.help_border))
                .title(" Dashboard ")
                .title_style(
                    Style::default()
                        .fg(theme.help_title)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(theme.help_bg).fg(theme.help_fg));

    frame.render_widget(dashboard, popup);
}

// ---------------------------------------------------------------------------
// Help overlay
// ---------------------------------------------------------------------------

/// Render a centered help overlay showing all keybindings.
fn render_help(theme: &Theme, frame: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 80, area);

    // Clear the area behind the popup.
    frame.render_widget(Clear, popup);

    // Key-description pairs for the help table, grouped by category.
    const KEY_COL_WIDTH: usize = 16;
    const GAP: usize = 2;

    let sections: Vec<(&str, Vec<(&str, &str)>)> = vec![
        (
            "Agent Management",
            vec![
                ("n", "New agent (quick launch)"),
                ("i / Enter", "Inline chat (queues if running)"),
                ("x", "Kill focused agent"),
                ("q", "Close focused tab"),
                ("Ctrl+C", "Quit all agents"),
            ],
        ),
        (
            "Navigation",
            vec![
                ("Tab / l", "Next agent"),
                ("Shift+Tab / h", "Previous agent"),
                ("1-9", "Focus agent by number"),
            ],
        ),
        (
            "Scrolling",
            vec![
                ("j / Down", "Scroll down"),
                ("k / Up", "Scroll up"),
                ("Ctrl+D / PgDn", "Half-page down"),
                ("Ctrl+U / PgUp", "Half-page up"),
                ("Ctrl+F", "Full-page down"),
                ("Ctrl+B", "Full-page up"),
                ("gg", "Scroll to top"),
                ("G", "Jump to bottom"),
            ],
        ),
        (
            "Views & Panels",
            vec![
                ("v", "Toggle dependency graph"),
                ("d", "Toggle aggregate dashboard"),
                ("|", "Toggle split-pane view"),
                ("`", "Switch split-pane focus"),
                ("r", "Cycle tool result display"),
                ("t", "Cycle color theme"),
            ],
        ),
        (
            "Tools",
            vec![
                (":", "Open command palette"),
                ("/", "Search output"),
                ("n / N", "Next/previous match (in search)"),
                ("y", "Copy output to clipboard"),
                ("e", "Export session to Markdown"),
                ("Enter", "Toggle tool result section"),
                ("?", "Toggle this help"),
            ],
        ),
    ];

    // Compute the maximum line width across all sections so every line
    // can be padded equally.
    let max_desc_len = sections
        .iter()
        .flat_map(|(_, bindings)| bindings.iter().map(|(_, d)| d.len()))
        .max()
        .unwrap_or(0);
    let max_line_width = KEY_COL_WIDTH + GAP + max_desc_len;

    // Build help lines with section headings and keybinding rows.
    let mut help_text: Vec<Line<'_>> = Vec::new();

    let heading_style = Style::default()
        .fg(theme.help_heading)
        .add_modifier(Modifier::BOLD);
    let footer_style = Style::default().fg(theme.help_footer);

    for (section_title, bindings) in &sections {
        // Blank line before each section heading.
        help_text.push(Line::from(" ".repeat(max_line_width)));
        help_text.push(help_line_padded(
            section_title,
            max_line_width,
            heading_style,
        ));
        help_text.push(Line::from(" ".repeat(max_line_width)));

        for (key, desc) in bindings {
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
    // strip_empty_text_lines tests
    // -----------------------------------------------------------------------

    #[test]
    fn strip_empty_preserves_non_empty_text_lines() {
        let lines = vec![
            DisplayLine::Text("hello".into()),
            DisplayLine::Text("world".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let result = strip_empty_text_lines(&refs);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn strip_empty_removes_empty_text_after_turn_start() {
        let lines = vec![
            DisplayLine::TurnStart {
                turn_number: Some(1),
            },
            DisplayLine::Text("".into()),
            DisplayLine::Text("".into()),
            DisplayLine::Text("hello".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let result = strip_empty_text_lines(&refs);
        // TurnStart + "hello" (two empty lines removed)
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], DisplayLine::TurnStart { .. }));
        assert!(matches!(result[1], DisplayLine::Text(ref s) if s == "hello"));
    }

    #[test]
    fn strip_empty_removes_empty_text_between_non_turn_start_lines() {
        let lines = vec![
            DisplayLine::Text("first".into()),
            DisplayLine::Text("".into()),
            DisplayLine::Text("second".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let result = strip_empty_text_lines(&refs);
        // Empty text lines are now always stripped (spacing is render-time).
        assert_eq!(result.len(), 2);
        assert!(matches!(result[0], DisplayLine::Text(ref s) if s == "first"));
        assert!(matches!(result[1], DisplayLine::Text(ref s) if s == "second"));
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
            DisplayLine::TurnStart {
                turn_number: Some(1),
            },
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
        let lines = render_to_spans(&blocks, &test_theme(), 80);
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
        let lines = render_to_spans(&blocks, &test_theme(), 80);
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
        let output = render_to_spans(&blocks, &test_theme(), 80);
        assert_eq!(output.len(), 2);
        // First line should have the connector prefix.
        let first_text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_text.contains('│'));
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
        let output = render_to_spans(&blocks, &test_theme(), 80);
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
        let output = render_to_spans(&blocks, &test_theme(), 80);
        assert_eq!(output.len(), 1);
    }

    #[test]
    fn render_text_run_adds_block_marker() {
        let blocks = vec![CollapsedBlock::TextRun("hello world".into())];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        assert!(!output.is_empty());
        let first_text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first_text.contains(BLOCK_MARKER));
        assert!(first_text.contains("hello world"));
    }

    #[test]
    fn render_single_system_line() {
        let dl = DisplayLine::System("system msg".into());
        let blocks = vec![CollapsedBlock::Single(&dl)];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        assert_eq!(output.len(), 1);
        let text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("system msg"));
    }

    #[test]
    fn render_single_error_line() {
        let dl = DisplayLine::Error("error msg".into());
        let blocks = vec![CollapsedBlock::Single(&dl)];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        assert_eq!(output.len(), 1);
        let text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("error msg"));
    }

    // -----------------------------------------------------------------------
    // Inter-element spacing tests
    // -----------------------------------------------------------------------

    /// Helper: check whether a Line is a blank line (empty or single empty span).
    fn is_blank_line(line: &Line<'_>) -> bool {
        line.spans.is_empty()
            || (line.spans.len() == 1 && line.spans[0].content.as_ref().is_empty())
    }

    #[test]
    fn spacing_blank_line_before_user_prompt() {
        // Rule 2: one blank line above a user prompt when preceded by text.
        let prompt_dl = DisplayLine::UserPrompt {
            content: "hello".into(),
            queued: false,
        };
        let blocks = vec![
            CollapsedBlock::TextRun("some text".into()),
            CollapsedBlock::Single(&prompt_dl),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // There should be a blank line between the text and the prompt.
        let blank_count = output.iter().filter(|l| is_blank_line(l)).count();
        assert!(
            blank_count >= 1,
            "expected at least one blank line before UserPrompt"
        );
        // Find the prompt line (contains ">").
        let prompt_idx = output
            .iter()
            .position(|l| l.spans.iter().any(|s| s.content.as_ref().contains('>')))
            .expect("should find user prompt line");
        assert!(prompt_idx > 0, "prompt should not be the first line");
        assert!(
            is_blank_line(&output[prompt_idx - 1]),
            "line before prompt should be blank"
        );
    }

    #[test]
    fn spacing_blank_line_after_user_prompt() {
        // Rule 2: one blank line below a user prompt.
        let prompt_dl = DisplayLine::UserPrompt {
            content: "hello".into(),
            queued: false,
        };
        let blocks = vec![
            CollapsedBlock::Single(&prompt_dl),
            CollapsedBlock::TextRun("response".into()),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // Find the prompt line.
        let prompt_idx = output
            .iter()
            .position(|l| l.spans.iter().any(|s| s.content.as_ref().contains('>')))
            .expect("should find user prompt line");
        // The next line after the prompt line(s) should be blank.
        let after_prompt = prompt_idx + 1;
        assert!(
            after_prompt < output.len(),
            "there should be lines after the prompt"
        );
        assert!(
            is_blank_line(&output[after_prompt]),
            "line after prompt should be blank"
        );
    }

    #[test]
    fn spacing_no_blank_between_consecutive_text_runs() {
        // Rule 3: no blank line between consecutive assistant text blocks.
        let blocks = vec![
            CollapsedBlock::TextRun("first paragraph".into()),
            CollapsedBlock::TextRun("second paragraph".into()),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // There should be no blank lines in the output.
        let blank_count = output.iter().filter(|l| is_blank_line(l)).count();
        assert_eq!(
            blank_count, 0,
            "no blank lines between consecutive text runs"
        );
    }

    #[test]
    fn spacing_blank_before_tool_call_after_text() {
        // Rule 4: one blank line before a tool call if preceded by assistant text.
        let tool_dl = DisplayLine::ToolUse {
            tool: "Read".into(),
            input_preview: "file.rs".into(),
        };
        let blocks = vec![
            CollapsedBlock::TextRun("some text".into()),
            CollapsedBlock::ToolUseHeader {
                dl: &tool_dl,
                effective_mode: ResultDisplay::Full,
            },
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // Find the tool use line (contains "Read").
        let tool_idx = output
            .iter()
            .position(|l| l.spans.iter().any(|s| s.content.as_ref().contains("Read")))
            .expect("should find tool use line");
        assert!(tool_idx > 0, "tool call should not be the first line");
        assert!(
            is_blank_line(&output[tool_idx - 1]),
            "line before tool call should be blank"
        );
    }

    #[test]
    fn spacing_blank_after_tool_result_run() {
        // Rule 5: one blank line after a tool result run before the next element.
        let blocks = vec![
            CollapsedBlock::ToolResultRun(ToolResultRun::Hidden { total_bytes: 10 }),
            CollapsedBlock::TextRun("next text".into()),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // The first line is the hidden result, then blank, then text.
        assert!(output.len() >= 3, "should have result + blank + text");
        assert!(
            is_blank_line(&output[1]),
            "second line should be blank (spacing after tool result)"
        );
    }

    #[test]
    fn spacing_blank_before_turn_summary() {
        // Rule 6: one blank line before turn summary.
        let summary_dl = DisplayLine::TurnSummary {
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.01,
        };
        let blocks = vec![
            CollapsedBlock::TextRun("content".into()),
            CollapsedBlock::Single(&summary_dl),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // Find the summary line (contains "in /").
        let summary_idx = output
            .iter()
            .position(|l| l.spans.iter().any(|s| s.content.as_ref().contains("in /")))
            .expect("should find turn summary line");
        assert!(summary_idx > 0, "summary should not be the first line");
        assert!(
            is_blank_line(&output[summary_idx - 1]),
            "line before summary should be blank"
        );
    }

    #[test]
    fn spacing_blank_around_thinking() {
        // Rule 7: one blank line above and below thinking blocks.
        let blocks = vec![
            CollapsedBlock::TextRun("before".into()),
            CollapsedBlock::ThinkingRun { line_count: 5 },
            CollapsedBlock::TextRun("after".into()),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // Find the thinking line (contains "Thinking").
        let thinking_idx = output
            .iter()
            .position(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.as_ref().contains("Thinking"))
            })
            .expect("should find thinking line");
        assert!(thinking_idx > 0, "thinking should not be the first line");
        assert!(
            is_blank_line(&output[thinking_idx - 1]),
            "line before thinking should be blank"
        );
        assert!(
            thinking_idx + 1 < output.len(),
            "there should be lines after thinking"
        );
        assert!(
            is_blank_line(&output[thinking_idx + 1]),
            "line after thinking should be blank"
        );
    }

    #[test]
    fn spacing_no_blank_around_system() {
        // Rule 8: system messages get no blank lines.
        let system_dl = DisplayLine::System("system msg".into());
        let blocks = vec![
            CollapsedBlock::TextRun("before".into()),
            CollapsedBlock::Single(&system_dl),
            CollapsedBlock::TextRun("after".into()),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // Find the system line.
        let system_idx = output
            .iter()
            .position(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.as_ref().contains("system msg"))
            })
            .expect("should find system line");
        // No blank line immediately before or after.
        if system_idx > 0 {
            assert!(
                !is_blank_line(&output[system_idx - 1]),
                "no blank line before system message"
            );
        }
        if system_idx + 1 < output.len() {
            assert!(
                !is_blank_line(&output[system_idx + 1]),
                "no blank line after system message"
            );
        }
    }

    #[test]
    fn spacing_blank_above_error_no_blank_below() {
        // Rule 9: one blank line above error, no blank line below.
        let error_dl = DisplayLine::Error("bad thing".into());
        let blocks = vec![
            CollapsedBlock::TextRun("before".into()),
            CollapsedBlock::Single(&error_dl),
            CollapsedBlock::TextRun("after".into()),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // Find the error line.
        let error_idx = output
            .iter()
            .position(|l| {
                l.spans
                    .iter()
                    .any(|s| s.content.as_ref().contains("bad thing"))
            })
            .expect("should find error line");
        assert!(error_idx > 0, "error should not be the first line");
        assert!(
            is_blank_line(&output[error_idx - 1]),
            "line before error should be blank"
        );
        // No blank line after error.
        if error_idx + 1 < output.len() {
            assert!(
                !is_blank_line(&output[error_idx + 1]),
                "no blank line after error"
            );
        }
    }

    #[test]
    fn spacing_no_double_blank_lines() {
        // Ensure that no matter the sequence, double blank lines never appear.
        let turn_dl = DisplayLine::TurnStart {
            turn_number: Some(1),
        };
        let prompt_dl = DisplayLine::UserPrompt {
            content: "test".into(),
            queued: false,
        };
        let summary_dl = DisplayLine::TurnSummary {
            input_tokens: 100,
            output_tokens: 50,
            cost_usd: 0.01,
        };
        let blocks = vec![
            CollapsedBlock::Single(&turn_dl),
            CollapsedBlock::Single(&prompt_dl),
            CollapsedBlock::TextRun("response".into()),
            CollapsedBlock::ThinkingRun { line_count: 3 },
            CollapsedBlock::TextRun("more text".into()),
            CollapsedBlock::Single(&summary_dl),
        ];
        let output = render_to_spans(&blocks, &test_theme(), 80);
        // Check no consecutive blank lines.
        for i in 1..output.len() {
            if is_blank_line(&output[i]) && is_blank_line(&output[i - 1]) {
                panic!(
                    "double blank lines at indices {} and {} (total {} lines)",
                    i - 1,
                    i,
                    output.len()
                );
            }
        }
    }

    #[test]
    fn dedup_blank_lines_collapses_consecutive() {
        let lines = vec![
            Line::from("content"),
            Line::from(""),
            Line::from(""),
            Line::from(""),
            Line::from("more"),
        ];
        let result = dedup_blank_lines(lines);
        assert_eq!(result.len(), 3); // content, blank, more
        assert!(!is_blank_line(&result[0]));
        assert!(is_blank_line(&result[1]));
        assert!(!is_blank_line(&result[2]));
    }

    #[test]
    fn dedup_blank_lines_trims_leading() {
        let lines = vec![Line::from(""), Line::from(""), Line::from("content")];
        let result = dedup_blank_lines(lines);
        assert_eq!(result.len(), 1);
        assert!(!is_blank_line(&result[0]));
    }

    // -----------------------------------------------------------------------
    // Full pipeline (collapse_tool_results) integration tests
    // -----------------------------------------------------------------------

    #[test]
    fn full_pipeline_empty_input() {
        let lines: Vec<&DisplayLine> = vec![];
        let no_overrides = std::collections::HashMap::new();
        let result = collapse_tool_results(
            &lines,
            ResultDisplay::Full,
            &no_overrides,
            &test_theme(),
            80,
        );
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
        let output = collapse_tool_results(
            &refs,
            ResultDisplay::Hidden,
            &no_overrides,
            &test_theme(),
            80,
        );
        assert_eq!(output.len(), 1);
        let text: String = output[0].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(text.contains("6 bytes"));
    }

    #[test]
    fn full_pipeline_strips_empty_after_turn_start() {
        let lines = vec![
            DisplayLine::TurnStart {
                turn_number: Some(1),
            },
            DisplayLine::Text("".into()),
            DisplayLine::Text("content".into()),
        ];
        let refs: Vec<&DisplayLine> = lines.iter().collect();
        let no_overrides = std::collections::HashMap::new();
        let output =
            collapse_tool_results(&refs, ResultDisplay::Full, &no_overrides, &test_theme(), 80);
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

    // -- format_cost ------------------------------------------------------

    #[test]
    fn format_cost_zero() {
        assert_eq!(format_cost(0.0), "$0.00");
    }

    #[test]
    fn format_cost_small() {
        assert_eq!(format_cost(0.15), "$0.15");
    }

    #[test]
    fn format_cost_under_hundred() {
        assert_eq!(format_cost(99.99), "$99.99");
    }

    #[test]
    fn format_cost_exactly_hundred() {
        assert_eq!(format_cost(100.0), "$100");
    }

    #[test]
    fn format_cost_hundreds() {
        assert_eq!(format_cost(123.45), "$123");
    }

    #[test]
    fn format_cost_thousands() {
        assert_eq!(format_cost(9999.0), "$9999");
    }

    #[test]
    fn format_cost_ten_thousand() {
        assert_eq!(format_cost(10_000.0), "$10.0K");
    }

    #[test]
    fn format_cost_large() {
        assert_eq!(format_cost(150_000.0), "$150K");
    }

    // -- format_token_count -----------------------------------------------

    #[test]
    fn format_token_count_zero() {
        assert_eq!(format_token_count(0), "0");
    }

    #[test]
    fn format_token_count_under_thousand() {
        assert_eq!(format_token_count(999), "999");
    }

    #[test]
    fn format_token_count_exactly_thousand() {
        assert_eq!(format_token_count(1000), "1.0K");
    }

    #[test]
    fn format_token_count_thousands() {
        assert_eq!(format_token_count(12500), "12.5K");
    }

    #[test]
    fn format_token_count_exactly_million() {
        assert_eq!(format_token_count(1_000_000), "1.0M");
    }

    #[test]
    fn format_token_count_millions() {
        assert_eq!(format_token_count(1_500_000), "1.5M");
    }

    // -----------------------------------------------------------------------
    // build_status_left_spans — abbreviated turn/tool counts (DKT-103)
    // -----------------------------------------------------------------------

    /// Helper: concatenate all span text content into a single string for assertions.
    fn spans_text(spans: &[Span<'_>]) -> String {
        spans.iter().map(|s| s.content.as_ref()).collect()
    }

    #[test]
    fn status_spans_width_50_abbreviated_turns_tools() {
        // Width 50 (45-59 range): should show abbreviated "3t" and "12T".
        let theme = test_theme();
        let spans = build_status_left_spans(
            50,
            &AgentActivity::Thinking,
            &AgentStatus::Running,
            3,
            12,
            "0:42",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(text.contains("3t"), "expected '3t' in: {text}");
        assert!(text.contains("12T"), "expected '12T' in: {text}");
        assert!(!text.contains("3 turns"), "should NOT contain full '3 turns' in: {text}");
        assert!(!text.contains("12 tools"), "should NOT contain full '12 tools' in: {text}");
    }

    #[test]
    fn status_spans_width_80_full_turns_tools() {
        // Width 80 (60+ range): should show full labels "3 turns" and "12 tools".
        let theme = test_theme();
        let spans = build_status_left_spans(
            80,
            &AgentActivity::Thinking,
            &AgentStatus::Running,
            3,
            12,
            "0:42",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(text.contains("3 turns"), "expected '3 turns' in: {text}");
        assert!(text.contains("12 tools"), "expected '12 tools' in: {text}");
    }

    #[test]
    fn status_spans_width_42_no_turns_tools() {
        // Width 42 (40-44 range): should NOT contain turn/tool text at all.
        let theme = test_theme();
        let spans = build_status_left_spans(
            42,
            &AgentActivity::Thinking,
            &AgentStatus::Running,
            3,
            12,
            "0:42",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(!text.contains("3t"), "should NOT contain '3t' in: {text}");
        assert!(!text.contains("12T"), "should NOT contain '12T' in: {text}");
        assert!(!text.contains("turns"), "should NOT contain 'turns' in: {text}");
        assert!(!text.contains("tools"), "should NOT contain 'tools' in: {text}");
    }

    #[test]
    fn status_spans_width_45_boundary_abbreviated() {
        // Width exactly 45: should show abbreviated format.
        let theme = test_theme();
        let spans = build_status_left_spans(
            45,
            &AgentActivity::Thinking,
            &AgentStatus::Running,
            1,
            1,
            "0:00",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(text.contains("1t"), "expected '1t' in: {text}");
        assert!(text.contains("1T"), "expected '1T' in: {text}");
        assert!(!text.contains("1 turn"), "should NOT contain '1 turn' in: {text}");
        assert!(!text.contains("1 tool"), "should NOT contain '1 tool' in: {text}");
    }

    #[test]
    fn status_spans_width_59_boundary_abbreviated() {
        // Width exactly 59: should still show abbreviated format.
        let theme = test_theme();
        let spans = build_status_left_spans(
            59,
            &AgentActivity::Thinking,
            &AgentStatus::Running,
            1,
            1,
            "0:00",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(text.contains("1t"), "expected '1t' in: {text}");
        assert!(text.contains("1T"), "expected '1T' in: {text}");
        assert!(!text.contains("1 turn"), "should NOT contain '1 turn' in: {text}");
        assert!(!text.contains("1 tool"), "should NOT contain '1 tool' in: {text}");
    }

    #[test]
    fn status_spans_width_60_boundary_full_labels() {
        // Width exactly 60: should show full labels with singular form.
        let theme = test_theme();
        let spans = build_status_left_spans(
            60,
            &AgentActivity::Thinking,
            &AgentStatus::Running,
            1,
            1,
            "0:00",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(text.contains("1 turn"), "expected '1 turn' in: {text}");
        assert!(text.contains("1 tool"), "expected '1 tool' in: {text}");
        // Ensure it is singular (not "turns"/"tools").
        assert!(!text.contains("1 turns"), "should NOT contain '1 turns' in: {text}");
        assert!(!text.contains("1 tools"), "should NOT contain '1 tools' in: {text}");
    }

    // -----------------------------------------------------------------------
    // build_status_left_spans — tool name truncation (DKT-104)
    // -----------------------------------------------------------------------

    #[test]
    fn status_spans_long_tool_name_truncated_at_wide_width() {
        // Width 80 (60+): tool name truncated to 20 chars + "...".
        let theme = test_theme();
        let long_name = "server:some_very_long_custom_tool_name";
        let spans = build_status_left_spans(
            80,
            &AgentActivity::Tool(long_name.to_string()),
            &AgentStatus::Running,
            3,
            12,
            "0:42",
            &theme,
        );
        let text = spans_text(&spans);
        // Should NOT contain the full tool name.
        assert!(!text.contains(long_name), "full tool name should be truncated in: {text}");
        // Should contain the truncated prefix (first 20 chars) + "...".
        let truncated_prefix: String = long_name.chars().take(20).collect();
        assert!(
            text.contains(&format!("{truncated_prefix}...")),
            "expected truncated '{truncated_prefix}...' in: {text}"
        );
    }

    #[test]
    fn status_spans_long_tool_name_truncated_at_narrow_width() {
        // Width 50 (45-59): tool name truncated to 12 chars + "...".
        let theme = test_theme();
        let long_name = "server:some_very_long_custom_tool_name";
        let spans = build_status_left_spans(
            50,
            &AgentActivity::Tool(long_name.to_string()),
            &AgentStatus::Running,
            3,
            12,
            "0:42",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(!text.contains(long_name), "full tool name should be truncated in: {text}");
        let truncated_prefix: String = long_name.chars().take(12).collect();
        assert!(
            text.contains(&format!("{truncated_prefix}...")),
            "expected truncated '{truncated_prefix}...' in: {text}"
        );
    }

    #[test]
    fn status_spans_short_tool_name_no_truncation() {
        // Width 80, short tool name "Bash": displayed in full, no truncation.
        let theme = test_theme();
        let spans = build_status_left_spans(
            80,
            &AgentActivity::Tool("Bash".to_string()),
            &AgentStatus::Running,
            3,
            12,
            "0:42",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(text.contains("Bash"), "expected 'Bash' in: {text}");
        assert!(!text.contains("..."), "should NOT contain '...' in: {text}");
    }

    #[test]
    fn status_spans_tool_name_exactly_at_limit_no_truncation() {
        // Width 80, tool name exactly 20 chars: displayed in full, no "...".
        let theme = test_theme();
        let name_20_chars = "abcdefghijklmnopqrst"; // exactly 20 chars
        assert_eq!(name_20_chars.len(), 20);
        let spans = build_status_left_spans(
            80,
            &AgentActivity::Tool(name_20_chars.to_string()),
            &AgentStatus::Running,
            3,
            12,
            "0:42",
            &theme,
        );
        let text = spans_text(&spans);
        assert!(text.contains(name_20_chars), "expected full tool name in: {text}");
        assert!(!text.contains("..."), "should NOT contain '...' in: {text}");
    }
}
