//! TUI widget rendering.
//!
//! Builds the ratatui widget tree from application state: tab bar, content
//! area with scrollable output, status bar, and help overlay.

use super::state::{AgentStatus, App, DisplayLine};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Tabs, Wrap};
use ratatui::Frame;

// ---------------------------------------------------------------------------
// Main render entry point
// ---------------------------------------------------------------------------

/// Render the entire TUI from application state.
///
/// Splits the terminal into three vertical sections (tab bar, content area,
/// status bar) and draws each one. If the help overlay is active it is drawn
/// on top of the full terminal area.
pub fn render(app: &App, frame: &mut Frame) {
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
}

// ---------------------------------------------------------------------------
// Tab bar
// ---------------------------------------------------------------------------

/// Render the tab bar showing one tab per agent.
fn render_tabs(app: &App, frame: &mut Frame, area: Rect) {
    if app.agents.is_empty() {
        let hint = Paragraph::new(" [No agents]")
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
            Line::from(format!(" {num}: {label} "))
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
                .title(" Agents ")
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
fn render_content(app: &App, frame: &mut Frame, area: Rect) {
    let block = Block::default().borders(Borders::NONE);

    let inner = block.inner(area);

    match app.focused_agent() {
        None => {
            let msg = Paragraph::new("No agent selected")
                .alignment(Alignment::Center)
                .style(Style::default().fg(Color::DarkGray))
                .block(block);
            frame.render_widget(msg, area);
        }
        Some(agent) => {
            let height = inner.height as usize;
            let visible = agent.visible_lines(height);

            if visible.is_empty() {
                let msg = Paragraph::new("Waiting for output...")
                    .alignment(Alignment::Center)
                    .style(Style::default().fg(Color::DarkGray))
                    .block(block);
                frame.render_widget(msg, area);
                return;
            }

            let lines: Vec<Line<'_>> = visible
                .iter()
                .map(|dl| display_line_to_line(dl, app.compact_results))
                .collect();

            let paragraph = Paragraph::new(lines)
                .block(block)
                .wrap(Wrap { trim: false });

            frame.render_widget(paragraph, area);
        }
    }
}

/// Maximum length of tool result content in compact mode.
const COMPACT_RESULT_MAX: usize = 200;

/// Convert a [`DisplayLine`] to a styled ratatui [`Line`].
fn display_line_to_line(dl: &DisplayLine, compact: bool) -> Line<'_> {
    match dl {
        DisplayLine::Text(s) => Line::from(Span::raw(s.as_str())),

        DisplayLine::ToolUse {
            tool,
            input_preview,
        } => Line::from(vec![
            Span::styled(
                format!("[{tool}] "),
                Style::default()
                    .fg(Color::Cyan)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(input_preview.as_str(), Style::default().fg(Color::Cyan)),
        ]),

        DisplayLine::ToolResult { content, is_error } => {
            let (label, label_color) = if *is_error {
                ("[ERROR] ", Color::Red)
            } else {
                ("[Result] ", Color::Green)
            };
            let display_content = if compact && content.len() > COMPACT_RESULT_MAX {
                let boundary = content
                    .char_indices()
                    .take_while(|&(i, _)| i < COMPACT_RESULT_MAX)
                    .last()
                    .map_or(0, |(i, c)| i + c.len_utf8());
                format!("{}…", &content[..boundary])
            } else {
                content.clone()
            };
            Line::from(vec![
                Span::raw("  "),
                Span::styled(
                    label,
                    Style::default()
                        .fg(label_color)
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(display_content, Style::default().fg(Color::DarkGray)),
            ])
        }

        DisplayLine::Thinking(s) => Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(Color::Magenta)
                .add_modifier(Modifier::DIM | Modifier::ITALIC),
        )),

        DisplayLine::Result(s) => Line::from(vec![
            Span::styled(
                "[Session] ",
                Style::default()
                    .fg(Color::Blue)
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(s.as_str(), Style::default().fg(Color::Blue)),
        ]),

        DisplayLine::System(s) => Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::ITALIC),
        )),

        DisplayLine::Stderr(s) => Line::from(Span::styled(
            s.as_str(),
            Style::default()
                .fg(Color::DarkGray)
                .add_modifier(Modifier::DIM),
        )),

        DisplayLine::Error(s) => Line::from(Span::styled(
            s.as_str(),
            Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
        )),
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
                let status_str = match &agent.status {
                    AgentStatus::Running => Span::styled(
                        "Running",
                        Style::default()
                            .fg(Color::Green)
                            .add_modifier(Modifier::BOLD),
                    ),
                    AgentStatus::Exited(Some(code)) => Span::styled(
                        format!("Exited ({code})"),
                        Style::default().fg(if *code == 0 {
                            Color::Yellow
                        } else {
                            Color::Red
                        }),
                    ),
                    AgentStatus::Exited(None) => {
                        Span::styled("Exited", Style::default().fg(Color::Yellow))
                    }
                };

                let line_count = agent.output.len();

                let scroll_info = if agent.scroll_offset == 0 {
                    "bottom".to_string()
                } else {
                    format!("-{}", agent.scroll_offset)
                };

                let mut spans = vec![
                    Span::raw(" "),
                    status_str,
                    Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("Lines: {line_count}")),
                    Span::styled(" | ", Style::default().fg(Color::DarkGray)),
                    Span::raw(format!("Scroll: {scroll_info}")),
                ];

                if agent.has_new_output {
                    spans.push(Span::styled(" | ", Style::default().fg(Color::DarkGray)));
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
        Span::styled(" Tab", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":switch  "),
        Span::styled("x", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":kill  "),
        Span::styled("r", Style::default().add_modifier(Modifier::BOLD)),
        Span::raw(":compact  "),
        Span::styled("q", Style::default().add_modifier(Modifier::BOLD)),
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
// Help overlay
// ---------------------------------------------------------------------------

/// Render a centered help overlay showing all keybindings.
fn render_help(frame: &mut Frame, area: Rect) {
    let popup = centered_rect(60, 70, area);

    // Clear the area behind the popup.
    frame.render_widget(Clear, popup);

    let help_text = vec![
        Line::from(Span::styled(
            "Keybindings",
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        help_line("Tab / l", "Next agent"),
        help_line("Shift+Tab / h", "Previous agent"),
        help_line("1-9", "Focus agent by number"),
        help_line("x", "Kill focused agent"),
        help_line("j / Down", "Scroll down (toward latest)"),
        help_line("k / Up", "Scroll up (into history)"),
        help_line("Ctrl+D / PgDn", "Half-page down"),
        help_line("Ctrl+U / PgUp", "Half-page up"),
        help_line("Ctrl+F", "Full-page down"),
        help_line("Ctrl+B", "Full-page up"),
        help_line("gg", "Scroll to top"),
        help_line("G", "Jump to bottom (latest)"),
        help_line("r", "Toggle compact results"),
        help_line("q / Ctrl+C", "Quit"),
        help_line("?", "Toggle this help"),
        Line::from(""),
        Line::from(Span::styled(
            "Press ? to close",
            Style::default().fg(Color::DarkGray),
        )),
    ];

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

/// Build a single help line with a key and description.
fn help_line<'a>(key: &'a str, desc: &'a str) -> Line<'a> {
    Line::from(vec![
        Span::styled(
            format!("{key:>16}"),
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::raw("  "),
        Span::raw(desc),
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
