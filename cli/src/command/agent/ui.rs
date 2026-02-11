//! TUI widget rendering.
//!
//! Builds the ratatui widget tree from application state: tab bar, content
//! area with scrollable output, status bar, and help overlay.

use super::state::{
    AgentActivity, AgentStatus, App, DisplayLine, InputField, InputMode, ResultDisplay,
    EFFORT_LEVELS, MODELS, PERMISSION_MODES,
};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
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

// ---------------------------------------------------------------------------
// Tool color mapping
// ---------------------------------------------------------------------------

/// Return a display color based on the tool category.
fn tool_color(tool: &str) -> Color {
    // Strip "server:" prefix for matching.
    let name = tool.strip_prefix("server:").unwrap_or(tool);
    match name {
        "Read" | "Grep" | "Glob" => Color::Green,
        "Write" | "Edit" | "NotebookEdit" => Color::Yellow,
        "Bash" => Color::Magenta,
        "WebSearch" | "WebFetch" | "web_search" => Color::Blue,
        _ => Color::Cyan,
    }
}

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

    // Gracefully handle absurdly small terminals — just clear and bail.
    if area.height < 5 || area.width < 10 {
        let msg = Paragraph::new("Terminal too small")
            .alignment(Alignment::Center)
            .style(Style::default().fg(Color::Red));
        frame.render_widget(msg, area);
        return;
    }

    let chunks = Layout::vertical([
        Constraint::Length(3), // tab bar
        Constraint::Fill(1),   // content
        Constraint::Length(2), // status bar
    ])
    .split(area);

    render_tabs(app, frame, chunks[0]);
    render_content(app, frame, chunks[1]);
    render_status(app, frame, chunks[2]);

    if app.show_help {
        render_help(frame, area);
    }

    if app.confirm_close {
        render_confirm_close(app, frame, area);
    }

    if app.input_mode == InputMode::Input {
        render_input(app, frame, area);
    }
}

// ---------------------------------------------------------------------------
// Tab bar
// ---------------------------------------------------------------------------

/// Render the tab bar showing one tab per agent with status badges.
fn render_tabs(app: &App, frame: &mut Frame, area: Rect) {
    if app.agents.is_empty() {
        let hint = Paragraph::new(" [No agents] — press 'n' to start one")
            .style(Style::default().fg(Color::DarkGray))
            .block(
                Block::default()
                    .borders(Borders::BOTTOM)
                    .border_style(Style::default().fg(Color::DarkGray)),
            );
        frame.render_widget(hint, area);
        return;
    }

    let titles: Vec<Line<'_>> = app
        .agents
        .iter()
        .enumerate()
        .map(|(i, agent)| {
            let label = workspace_label(&agent.workspace);
            let num = i + 1;
            let (badge, badge_color) = match (&agent.status, &agent.activity) {
                (AgentStatus::Exited(Some(0)), _) => (CHECK_MARK, Color::Green),
                (AgentStatus::Exited(_), _) => (CROSS_MARK, Color::Red),
                (AgentStatus::Running, AgentActivity::Idle) => (CIRCLE, Color::DarkGray),
                (AgentStatus::Running, AgentActivity::Done) => (CHECK_MARK, Color::Green),
                (AgentStatus::Running, _) => {
                    let frame_idx = app.tick % SPINNER_FRAMES.len();
                    (SPINNER_FRAMES[frame_idx], Color::Cyan)
                }
            };
            Line::from(vec![
                Span::styled(format!(" {badge}"), Style::default().fg(badge_color)),
                Span::raw(format!(" {num}: ⌂ {label} ")),
            ])
        })
        .collect();

    let tabs = Tabs::new(titles)
        .select(app.focused)
        .highlight_style(
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD | Modifier::REVERSED),
        )
        .style(Style::default().fg(Color::White))
        .divider(Span::raw("|"))
        .block(
            Block::default()
                .borders(Borders::BOTTOM)
                .border_style(Style::default().fg(Color::DarkGray))
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
                        .fg(Color::White)
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

// ---------------------------------------------------------------------------
// Content area
// ---------------------------------------------------------------------------

/// Render the main content area with the focused agent's output.
fn render_content(app: &mut App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::NONE);

    let inner = block.inner(area);

    match app.focused_agent() {
        None => {
            let msg = Paragraph::new("No agents yet — press 'n' to start one")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(msg, area);
        }
        Some(_) => {
            let height = inner.height as usize;
            let focused = app.focused;
            let agent = &app.agents[focused];

            if agent.output.is_empty() {
                let msg = Paragraph::new("Waiting for output...")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray))
                    .block(block);
                frame.render_widget(msg, area);
                return;
            }

            // Check if cached data is still valid.
            let cache_hit = agent.cached_lines.is_some()
                && agent.cache_generation == agent.output_generation
                && agent.cache_result_display == Some(app.result_display);

            if !cache_hit {
                // Recompute collapsed lines and row count.
                let all_lines: Vec<&DisplayLine> = agent.output.iter().collect();
                let collapsed = collapse_tool_results(&all_lines, app.result_display);
                let row_count = wrapped_row_count(&collapsed, inner.width);
                let owned = lines_to_static(collapsed);

                let agent_mut = &mut app.agents[focused];
                agent_mut.cached_lines = Some(owned);
                agent_mut.cached_row_count = row_count;
                agent_mut.cache_generation = agent_mut.output_generation;
                agent_mut.cache_result_display = Some(app.result_display);
            }

            let agent = &app.agents[focused];
            let cached = agent.cached_lines.as_ref().unwrap();
            let total_rows = agent.cached_row_count;

            let max_scroll = total_rows.saturating_sub(height);

            // scroll_offset 0 = pinned to bottom (latest output).
            // Convert to scroll-from-top for Paragraph::scroll().
            let clamped_offset = agent.scroll_offset.min(max_scroll);
            let scroll_y = max_scroll.saturating_sub(clamped_offset) as u16;

            let paragraph = Paragraph::new(cached.clone())
                .block(block)
                .wrap(Wrap { trim: false })
                .scroll((scroll_y, 0));

            frame.render_widget(paragraph, area);
        }
    }
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

/// Maximum number of consecutive tool result lines shown before collapsing.
const COMPACT_RESULT_RUN_MAX: usize = 3;

/// Collapse consecutive runs of [`DisplayLine::ToolResult`] based on display mode.
///
/// - **Full**: all lines shown with full content.
/// - **Compact**: runs longer than [`COMPACT_RESULT_RUN_MAX`] are collapsed;
///   individual lines are truncated to [`COMPACT_RESULT_MAX`] chars.
/// - **Hidden**: entire result runs are replaced with a single byte-count summary.
///
/// In all modes, consecutive tool result runs are tracked so that only the
/// first line displays the `⎿` connector; continuation lines use matching
/// whitespace indentation (Claude Code style).
fn collapse_tool_results<'a>(lines: &[&'a DisplayLine], mode: ResultDisplay) -> Vec<Line<'a>> {
    // Strip leading empty Text lines that immediately follow a TurnStart.
    // Claude often starts a response with "\n\n" which produces empty Text("")
    // lines — these look fine mid-conversation but create visual noise at the
    // start of the first turn.
    let stripped = strip_leading_empty_after_turn_start(lines);
    let lines = &stripped;

    let mut out = Vec::with_capacity(lines.len());
    let mut i = 0;

    while i < lines.len() {
        if matches!(lines[i], DisplayLine::ToolResult { .. }) {
            // Measure the consecutive ToolResult run. Each entry may
            // contain multiple newline-separated lines of content.
            let run_start = i;
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

            match mode {
                ResultDisplay::Hidden => {
                    // Replace the entire run with a byte-count summary.
                    let size = if total_bytes >= 1024 {
                        format!("{:.1} KB", total_bytes as f64 / 1024.0)
                    } else {
                        format!("{total_bytes} bytes")
                    };
                    out.push(Line::from(Span::styled(
                        format!("  {RESULT_CONNECTOR}  {size} (press 'r' to cycle view)"),
                        Style::default().fg(Color::Gray),
                    )));
                }
                ResultDisplay::Compact => {
                    // Show up to COMPACT_RESULT_RUN_MAX lines across all
                    // entries in the run, then collapse the rest.
                    let mut lines_shown: usize = 0;
                    let mut first_line_overall = true;
                    for dl in &lines[run_start..i] {
                        if let DisplayLine::ToolResult {
                            content, is_error, ..
                        } = dl
                        {
                            for text_line in content.lines() {
                                if lines_shown >= COMPACT_RESULT_RUN_MAX {
                                    break;
                                }
                                out.extend(render_tool_result(
                                    text_line,
                                    *is_error,
                                    mode,
                                    first_line_overall,
                                ));
                                first_line_overall = false;
                                lines_shown += 1;
                            }
                        }
                        if lines_shown >= COMPACT_RESULT_RUN_MAX {
                            break;
                        }
                    }
                    let hidden = total_lines.saturating_sub(COMPACT_RESULT_RUN_MAX);
                    if hidden > 0 {
                        out.push(Line::from(Span::styled(
                            format!(
                                "  {RESULT_CONNECTOR}  ... {hidden} more lines hidden (press 'r' to cycle view)"
                            ),
                            Style::default().fg(Color::Gray),
                        )));
                    }
                }
                ResultDisplay::Full => {
                    let mut first_line_overall = true;
                    for dl in &lines[run_start..i] {
                        if let DisplayLine::ToolResult {
                            content, is_error, ..
                        } = dl
                        {
                            for text_line in content.lines() {
                                out.extend(render_tool_result(
                                    text_line,
                                    *is_error,
                                    mode,
                                    first_line_overall,
                                ));
                                first_line_overall = false;
                            }
                        }
                    }
                }
            }
        } else if matches!(lines[i], DisplayLine::Text(_)) {
            // Gather consecutive Text lines into a single markdown string.
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
            let preprocessed = preprocess_markdown_tables(&markdown);
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
                        Span::styled(format!("{BLOCK_MARKER} "), Style::default().fg(Color::Cyan))
                    } else {
                        Span::raw(CONTINUATION_INDENT)
                    };
                    out.push(Line::from(vec![
                        prefix,
                        Span::styled(
                            "──────────────────────────────",
                            Style::default().fg(Color::DarkGray),
                        ),
                    ]));
                } else if line.width() == 0 {
                    out.push(Line::from(""));
                } else if need_marker {
                    need_marker = false;
                    let mut spans: Vec<Span<'static>> = vec![Span::styled(
                        format!("{BLOCK_MARKER} "),
                        Style::default().fg(Color::Cyan),
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
        } else {
            out.extend(display_line_to_lines(lines[i]));
            i += 1;
        }
    }

    out
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
            Style::default().fg(Color::DarkGray),
        )
    } else {
        Span::raw(CONTINUATION_INDENT)
    };

    let mut spans = vec![prefix];
    if is_error {
        spans.push(Span::styled(
            "[ERROR] ",
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ));
    }
    spans.push(Span::styled(
        display_content,
        Style::default().fg(Color::Gray),
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

/// Convert a [`DisplayLine`] to one or more styled ratatui [`Line`]s.
///
/// Handles all variants except [`DisplayLine::Text`] and
/// [`DisplayLine::ToolResult`], which are rendered inline (via
/// `tui_markdown`) and by [`render_tool_result`] respectively with
/// run-position awareness.
fn display_line_to_lines(dl: &DisplayLine) -> Vec<Line<'_>> {
    match dl {
        // Text is handled by render_text_line() for run tracking.
        DisplayLine::Text(_) => Vec::new(),

        DisplayLine::ToolUse {
            tool,
            input_preview,
        } => {
            let color = tool_color(tool);
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
                    Style::default().fg(Color::DarkGray),
                ));
            }
            vec![Line::from(spans)]
        }

        // ToolResult is handled by render_tool_result() for run tracking.
        DisplayLine::ToolResult { .. } => Vec::new(),

        DisplayLine::Thinking(s) => vec![Line::from(vec![
            Span::styled(
                format!("{BLOCK_MARKER} "),
                Style::default().fg(Color::Magenta),
            ),
            Span::styled(
                s.as_str(),
                Style::default()
                    .fg(Color::Magenta)
                    .add_modifier(Modifier::DIM | Modifier::ITALIC),
            ),
        ])],

        DisplayLine::Result(s) => vec![Line::from(vec![
            Span::styled(
                format!("{SESSION_MARKER} "),
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(s.as_str(), Style::default().fg(Color::Blue)),
        ])],

        DisplayLine::System(s) => vec![Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        ))],

        DisplayLine::Stderr(s) => vec![Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        ))],

        DisplayLine::Error(s) => vec![Line::from(Span::styled(
            s.as_str(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        ))],

        DisplayLine::TurnStart => vec![Line::from("")],
    }
}

// ---------------------------------------------------------------------------
// Status bar
// ---------------------------------------------------------------------------

/// Render the two-line status bar at the bottom.
fn render_status(app: &App, frame: &mut Frame, area: Rect) {
    // Split the 2-line area into two 1-line rows.
    let rows = Layout::vertical([Constraint::Length(1), Constraint::Length(1)]).split(area);

    // Line 1: transient status message (if active) or agent status info.
    let status_line = if let Some(msg) = app.status_message() {
        Line::from(vec![
            Span::raw(" "),
            Span::styled(
                msg,
                Style::default()
                    .fg(Color::Yellow)
                    .add_modifier(Modifier::BOLD),
            ),
        ])
    } else {
        match app.focused_agent() {
            None => Line::from(Span::styled(
                " No agent",
                Style::default().fg(Color::DarkGray),
            )),
            Some(agent) => {
                let (activity_label, activity_color) = match &agent.activity {
                    AgentActivity::Idle => ("Idle", Color::DarkGray),
                    AgentActivity::Thinking => ("Thinking", Color::Yellow),
                    AgentActivity::Tool(_) => {
                        // Handled specially below for the formatted span.
                        ("", Color::Cyan)
                    }
                    AgentActivity::Done => ("Done", Color::Green),
                };

                let activity_span = if let AgentActivity::Tool(name) = &agent.activity {
                    Span::styled(
                        format!("{ARROW_RIGHT} {name}"),
                        Style::default()
                            .fg(Color::Cyan)
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

                let sep = Span::styled(" | ", Style::default().fg(Color::DarkGray));

                let scroll_info = if agent.scroll_offset == 0 {
                    "bottom".to_string()
                } else {
                    format!("-{}", agent.scroll_offset)
                };

                let mut spans = vec![
                    Span::raw(" "),
                    activity_span,
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
                            .fg(Color::Yellow)
                            .add_modifier(Modifier::BOLD),
                    ));
                }

                Line::from(spans)
            }
        }
    };

    let status_paragraph =
        Paragraph::new(status_line).style(Style::default().bg(Color::DarkGray).fg(Color::White));
    frame.render_widget(status_paragraph, rows[0]);

    // Line 2: keybinding hints
    let hints = Line::from(vec![
        Span::styled(" n", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":new  "),
        Span::styled("s", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":respond  "),
        Span::styled("Tab", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":switch  "),
        Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":kill  "),
        Span::styled("y", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":copy  "),
        Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":results  "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":close  "),
        Span::styled("^C", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":quit  "),
        Span::styled("j/k", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":scroll  "),
        Span::styled("^D/^U", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":page  "),
        Span::styled("gg", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":top  "),
        Span::styled("G", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":bottom  "),
        Span::styled("?", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":help"),
    ]);

    let hints_paragraph =
        Paragraph::new(hints).style(Style::default().bg(Color::Black).fg(Color::DarkGray));
    frame.render_widget(hints_paragraph, rows[1]);
}

// ---------------------------------------------------------------------------
// Input overlay
// ---------------------------------------------------------------------------

/// Return the label style for a field — cyan/bold when active, dimmed otherwise.
fn field_label_style(active: bool) -> Style {
    if active {
        Style::default()
            .fg(Color::Cyan)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().fg(Color::DarkGray)
    }
}

/// Render a centered input overlay for entering a new agent prompt, workspace,
/// and Claude Code options.
fn render_input(app: &App, frame: &mut Frame, area: Rect) {
    // Compute inner width from the percentage-based horizontal layout so we
    // can calculate how many rows the input text will wrap to. The inner
    // content width is the popup width minus 2 (left + right border).
    let popup_width = (area.width as u32 * 60 / 100) as usize;
    let inner_width = popup_width.saturating_sub(2);

    // Count wrapped rows for each input field.
    let prompt_rows = wrapped_input_rows(&app.input_buffer, inner_width);
    let workspace_rows = wrapped_input_rows(&app.workspace_buffer, inner_width);

    // Layout: label + prompt + blank + label + workspace + blank
    //       + header + 6×(label + value) + blank + footer
    let option_rows = 1 + 6 * 2; // header + 6 fields × (label + value)
    let content_rows = 1 + prompt_rows + 1 + 1 + workspace_rows + 1 + option_rows + 1 + 1;
    let popup_height = (content_rows + 2).min(area.height as usize) as u16;

    let popup = centered_rect_fixed_height(60, popup_height, area);

    // Clear the area behind the popup.
    frame.render_widget(Clear, popup);

    let inner_height = popup.height.saturating_sub(2) as usize;

    // Build input lines — only the active field shows a cursor.
    let prompt_lines = build_input_field_lines(
        &app.input_buffer,
        app.input_cursor,
        app.input_field == InputField::Prompt,
    );
    let workspace_lines = build_input_field_lines(
        &app.workspace_buffer,
        app.workspace_cursor,
        app.input_field == InputField::Workspace,
    );

    // Claude option fields — selector fields use build_selector, others show
    // "(unset)" placeholder when empty and inactive.
    let perm_lines = build_selector(
        PERMISSION_MODES,
        &app.permission_mode_buffer,
        app.input_field == InputField::PermissionMode,
    );
    let model_lines = build_selector(
        MODELS,
        &app.model_buffer,
        app.input_field == InputField::Model,
    );
    let effort_lines = build_selector(
        EFFORT_LEVELS,
        &app.effort_buffer,
        app.input_field == InputField::Effort,
    );
    let budget_lines = build_optional_field_lines(
        &app.max_budget_buffer,
        app.max_budget_cursor,
        app.input_field == InputField::MaxBudgetUsd,
    );
    let tools_lines = build_optional_field_lines(
        &app.allowed_tools_buffer,
        app.allowed_tools_cursor,
        app.input_field == InputField::AllowedTools,
    );
    let dir_lines = build_optional_field_lines(
        &app.add_dir_buffer,
        app.add_dir_cursor,
        app.input_field == InputField::AddDir,
    );

    // Footer hint varies by active field.
    let footer = match app.input_field {
        InputField::Prompt => "Enter: newline  |  ^S: submit  |  Tab: next  |  Esc: cancel",
        InputField::PermissionMode | InputField::Model | InputField::Effort => {
            "◀/▶: select  |  Enter/Tab: next  |  ^S: submit  |  Esc: cancel"
        }
        _ => "Enter/Tab: next  |  ^S: submit  |  Esc: cancel",
    };

    let mut text = Vec::new();
    text.push(Line::from(Span::styled(
        "Prompt:",
        field_label_style(app.input_field == InputField::Prompt),
    )));
    text.extend(prompt_lines);
    text.push(Line::from(""));
    text.push(Line::from(Span::styled(
        "Workspace:",
        field_label_style(app.input_field == InputField::Workspace),
    )));
    text.extend(workspace_lines);
    text.push(Line::from(""));

    // Claude options header.
    text.push(Line::from(Span::styled(
        "── Claude Options ──",
        Style::default()
            .fg(Color::DarkGray)
            .add_modifier(Modifier::DIM),
    )));

    text.push(Line::from(Span::styled(
        "Permission Mode:",
        field_label_style(app.input_field == InputField::PermissionMode),
    )));
    text.extend(perm_lines);
    text.push(Line::from(Span::styled(
        "Model:",
        field_label_style(app.input_field == InputField::Model),
    )));
    text.extend(model_lines);
    text.push(Line::from(Span::styled(
        "Effort:",
        field_label_style(app.input_field == InputField::Effort),
    )));
    text.extend(effort_lines);
    text.push(Line::from(Span::styled(
        "Max Budget USD:",
        field_label_style(app.input_field == InputField::MaxBudgetUsd),
    )));
    text.extend(budget_lines);
    text.push(Line::from(Span::styled(
        "Allowed Tools (comma-separated):",
        field_label_style(app.input_field == InputField::AllowedTools),
    )));
    text.extend(tools_lines);
    text.push(Line::from(Span::styled(
        "Add Dir (comma-separated):",
        field_label_style(app.input_field == InputField::AddDir),
    )));
    text.extend(dir_lines);

    text.push(Line::from(""));
    text.push(
        Line::from(Span::styled(footer, Style::default().fg(Color::DarkGray)))
            .alignment(Alignment::Center),
    );

    // Scroll so the cursor in the active field stays visible.
    //
    // Compute the wrapped row the cursor occupies, accounting for explicit
    // newlines in the buffer.
    let (active_buffer, active_cursor) = match app.input_field {
        InputField::Prompt => (&app.input_buffer, app.input_cursor),
        InputField::Workspace => (&app.workspace_buffer, app.workspace_cursor),
        InputField::PermissionMode => (&app.permission_mode_buffer, app.permission_mode_cursor),
        InputField::Model => (&app.model_buffer, app.model_cursor),
        InputField::Effort => (&app.effort_buffer, app.effort_cursor),
        InputField::MaxBudgetUsd => (&app.max_budget_buffer, app.max_budget_cursor),
        InputField::AllowedTools => (&app.allowed_tools_buffer, app.allowed_tools_cursor),
        InputField::AddDir => (&app.add_dir_buffer, app.add_dir_cursor),
    };
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
    let cursor_absolute_row = {
        // Rows consumed by: Prompt label(1) + prompt_rows + blank(1) +
        //                   Workspace label(1) + workspace_rows + blank(1) +
        //                   header(1) + ... option fields ...
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

    let input_widget = Paragraph::new(text)
        .wrap(Wrap { trim: false })
        .scroll((scroll_y, 0))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(if let Some(target_id) = app.respond_target {
                    let label = app
                        .agent_vec_index(target_id)
                        .map(|idx| format!("{}", idx + 1))
                        .unwrap_or_else(|| {
                            tracing::debug!(target_id, "respond target has no Vec index");
                            "?".to_string()
                        });
                    format!(" Respond to Agent {} ", label)
                } else {
                    " New Agent ".to_string()
                })
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(Color::Black).fg(Color::White));

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
fn build_selector(options: &[&str], current: &str, active: bool) -> Vec<Line<'static>> {
    let mut spans: Vec<Span<'static>> = Vec::new();

    for (i, option) in options.iter().enumerate() {
        if i > 0 {
            spans.push(Span::styled(" │ ", Style::default().fg(Color::DarkGray)));
        }

        let is_selected = *option == current;

        if is_selected {
            spans.push(Span::styled(
                option.to_string(),
                Style::default()
                    .fg(if active { Color::Cyan } else { Color::White })
                    .add_modifier(Modifier::BOLD | Modifier::REVERSED),
            ));
        } else {
            spans.push(Span::styled(
                option.to_string(),
                Style::default().fg(Color::DarkGray),
            ));
        }
    }

    vec![Line::from(spans)]
}

/// Build input field lines for an optional Claude option.
///
/// Shows a dim "(unset)" placeholder when the buffer is empty and the field is
/// not active. Otherwise delegates to [`build_input_field_lines`].
fn build_optional_field_lines(buffer: &str, cursor: usize, active: bool) -> Vec<Line<'static>> {
    if buffer.is_empty() && !active {
        vec![Line::from(Span::styled(
            "(unset)",
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        ))]
    } else {
        build_input_field_lines(buffer, cursor, active)
    }
}

/// Build one or more input field lines with optional cursor highlight.
///
/// Splits the buffer on `\n` so that multiline content (e.g. the prompt
/// field) renders correctly. The cursor highlight block is placed on the
/// appropriate segment.
fn build_input_field_lines(buffer: &str, cursor: usize, active: bool) -> Vec<Line<'static>> {
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
                    Style::default().bg(Color::White).fg(Color::Black),
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
// Confirm-close overlay
// ---------------------------------------------------------------------------

/// Render a confirmation dialog when the user tries to close a running agent.
fn render_confirm_close(app: &App, frame: &mut Frame, area: Rect) {
    let popup = centered_rect(50, 30, area);
    frame.render_widget(Clear, popup);

    let agent_num = app.focused + 1;
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            format!("Agent {agent_num} is still running."),
            Style::default()
                .fg(Color::Yellow)
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
                    .fg(Color::Green)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("/"),
            Span::styled(
                "n",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ),
        ])
        .alignment(Alignment::Center),
        Line::from(""),
    ];

    let confirm = Paragraph::new(text)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Yellow))
                .title(" Confirm Close ")
                .title_style(
                    Style::default()
                        .fg(Color::Yellow)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(Color::Black).fg(Color::White));

    frame.render_widget(confirm, popup);
}

// ---------------------------------------------------------------------------
// Help overlay
// ---------------------------------------------------------------------------

/// Render a centered help overlay showing all keybindings.
fn render_help(frame: &mut Frame, area: Rect) {
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
        ("n", "New agent (prompt + workspace)"),
        ("s", "Respond to exited agent (session)"),
        ("x", "Kill focused agent"),
        ("y", "Copy output to clipboard"),
        ("j / Down", "Scroll down (toward latest)"),
        ("k / Up", "Scroll up (into history)"),
        ("Ctrl+D / PgDn", "Half-page down"),
        ("Ctrl+U / PgUp", "Half-page up"),
        ("Ctrl+F", "Full-page down"),
        ("Ctrl+B", "Full-page up"),
        ("gg", "Scroll to top"),
        ("G", "Jump to bottom (latest)"),
        ("r", "Toggle compact results"),
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

    help_text.push(help_line_padded("Keybindings", max_line_width));
    help_text.push(Line::from(" ".repeat(max_line_width)));

    for (key, desc) in &bindings {
        let key_span = Span::styled(
            format!("{key:>width$}", width = KEY_COL_WIDTH),
            Style::default()
                .fg(Color::Yellow)
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
    help_text.push(help_line_padded("Press ? to close", max_line_width));

    let help = Paragraph::new(help_text)
        .alignment(Alignment::Center)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Style::default().fg(Color::Cyan))
                .title(" Help ")
                .title_style(
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ),
        )
        .style(Style::default().bg(Color::Black).fg(Color::White));

    frame.render_widget(help, popup);
}

/// Build a centered label line padded to `width` so it aligns with the
/// keybinding block when `Alignment::Center` is applied.
fn help_line_padded<'a>(text: &'a str, width: usize) -> Line<'a> {
    let text_len = text.len();
    let total_pad = width.saturating_sub(text_len);
    let left_pad = total_pad / 2;
    let right_pad = total_pad - left_pad;
    Line::from(vec![
        Span::raw(" ".repeat(left_pad)),
        Span::styled(
            text,
            if text == "Press ? to close" {
                Style::default().fg(Color::DarkGray)
            } else {
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD)
            },
        ),
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
