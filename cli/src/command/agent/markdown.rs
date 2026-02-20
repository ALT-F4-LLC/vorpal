//! High-fidelity Markdown -> ratatui renderer.
//!
//! Uses pulldown-cmark for parsing and syntect for syntax highlighting.
//! Replaces the previous tui-markdown (v0.3) dependency with a custom
//! pipeline that supports syntax-highlighted code blocks, proper tables,
//! inline code styling, and rich formatting.

use std::sync::OnceLock;

use pulldown_cmark::{CodeBlockKind, Event, Options, Parser, Tag, TagEnd};
use ratatui::{
    style::{Color, Modifier, Style},
    text::{Line, Span},
};
use syntect::{
    easy::HighlightLines,
    highlighting::{FontStyle, ThemeSet},
    parsing::SyntaxSet,
};
use unicode_width::UnicodeWidthStr;

use super::theme::Theme;

/// Width of code block border lines (in `─` characters).
const CODE_BORDER_WIDTH: usize = 34;
/// Width of the `───` prefix before the language label in fenced code blocks.
const CODE_LABEL_PREFIX_WIDTH: usize = 3;

// ---------------------------------------------------------------------------
// Lazy-loaded syntect assets
// ---------------------------------------------------------------------------

fn syntax_set() -> &'static SyntaxSet {
    static SS: OnceLock<SyntaxSet> = OnceLock::new();
    SS.get_or_init(SyntaxSet::load_defaults_newlines)
}

fn theme_set() -> &'static ThemeSet {
    static TS: OnceLock<ThemeSet> = OnceLock::new();
    TS.get_or_init(ThemeSet::load_defaults)
}

// ---------------------------------------------------------------------------
// syntect -> ratatui style translation
// ---------------------------------------------------------------------------

fn translate_color(c: syntect::highlighting::Color) -> Color {
    Color::Rgb(c.r, c.g, c.b)
}

fn translate_style(style: syntect::highlighting::Style) -> Style {
    let mut ratatui_style = Style::default().fg(translate_color(style.foreground));
    if style.font_style.contains(FontStyle::BOLD) {
        ratatui_style = ratatui_style.add_modifier(Modifier::BOLD);
    }
    if style.font_style.contains(FontStyle::ITALIC) {
        ratatui_style = ratatui_style.add_modifier(Modifier::ITALIC);
    }
    if style.font_style.contains(FontStyle::UNDERLINE) {
        ratatui_style = ratatui_style.add_modifier(Modifier::UNDERLINED);
    }
    ratatui_style
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Render markdown text into ratatui [`Line`]s.
///
/// This is the main entry point replacing `tui_markdown::from_str()`.
/// It handles code blocks with syntax highlighting, inline formatting,
/// lists, blockquotes, headings, links, and horizontal rules.
///
/// Tables are rendered with box-drawing borders and aligned columns.
pub fn render_markdown(input: &str, theme: &Theme) -> Vec<Line<'static>> {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);

    let parser = Parser::new_ext(input, opts);
    let mut renderer = MarkdownRenderer::new(theme);
    renderer.render(parser);
    renderer.lines
}

// ---------------------------------------------------------------------------
// Internal renderer
// ---------------------------------------------------------------------------

/// A single table cell with styled spans and its display width.
struct TableCell {
    spans: Vec<Span<'static>>,
    display_width: usize,
}

struct MarkdownRenderer<'t> {
    theme: &'t Theme,
    lines: Vec<Line<'static>>,
    /// Current line being built (spans accumulate here).
    current_spans: Vec<Span<'static>>,
    /// Stack of active modifiers (bold, italic, etc.).
    style_stack: Vec<Style>,
    /// Are we inside a code block? If so, what language?
    code_block: Option<String>,
    /// Accumulated text for the current code block.
    code_text: String,
    /// Current list nesting with item counters (None = unordered, Some(n) = ordered starting at n).
    list_stack: Vec<Option<u64>>,
    /// Current item index within each list level.
    list_counters: Vec<u64>,
    /// Blockquote nesting depth (0 = not in a blockquote).
    blockquote_depth: usize,
    /// Accumulated table row cells.
    table_row: Vec<TableCell>,
    /// Table data: rows of cells. First row is the header.
    table_rows: Vec<Vec<TableCell>>,
    /// Column alignments from the Table tag.
    table_alignments: Vec<pulldown_cmark::Alignment>,
}

impl<'t> MarkdownRenderer<'t> {
    fn new(theme: &'t Theme) -> Self {
        Self {
            theme,
            lines: Vec::new(),
            current_spans: Vec::new(),
            style_stack: vec![Style::default()],
            code_block: None,
            code_text: String::new(),
            list_stack: Vec::new(),
            list_counters: Vec::new(),
            blockquote_depth: 0,
            table_row: Vec::new(),
            table_rows: Vec::new(),
            table_alignments: Vec::new(),
        }
    }

    fn render<'a>(&mut self, parser: Parser<'a>) {
        for event in parser {
            match event {
                // -- Block-level tags ----------------------------------------
                Event::Start(Tag::Paragraph) => {}
                Event::End(TagEnd::Paragraph) => {
                    self.flush_line();
                    self.lines.push(Line::from(""));
                }

                Event::Start(Tag::Heading { .. }) => {
                    let heading_style = self.current_style()
                        .fg(self.theme.heading_fg)
                        .add_modifier(Modifier::BOLD);
                    self.style_stack.push(heading_style);
                }
                Event::End(TagEnd::Heading(_)) => {
                    self.flush_line();
                    self.pop_modifier();
                }

                Event::Start(Tag::CodeBlock(kind)) => {
                    let lang = match &kind {
                        CodeBlockKind::Fenced(lang) => {
                            let l = lang.split_whitespace().next().unwrap_or("");
                            l.to_string()
                        }
                        CodeBlockKind::Indented => String::new(),
                    };
                    self.code_block = Some(lang);
                    self.code_text.clear();
                }
                Event::End(TagEnd::CodeBlock) => {
                    self.render_code_block();
                    self.code_text.clear();
                    self.code_block = None;
                }

                Event::Start(Tag::List(first_item)) => {
                    self.list_stack.push(first_item);
                    self.list_counters.push(first_item.unwrap_or(0));
                }
                Event::End(TagEnd::List(_)) => {
                    self.list_stack.pop();
                    self.list_counters.pop();
                }

                Event::Start(Tag::Item) => {
                    let indent = "  ".repeat(self.list_stack.len().saturating_sub(1));
                    let marker_style = Style::default().fg(self.theme.list_marker_fg);
                    let marker = if let Some(Some(_start)) =
                        self.list_stack.last()
                    {
                        let counter =
                            self.list_counters.last_mut().expect("counter exists");
                        let m = format!("{}{}. ", indent, counter);
                        *counter += 1;
                        m
                    } else {
                        format!("{}- ", indent)
                    };
                    self.current_spans
                        .push(Span::styled(marker, marker_style));
                }
                Event::End(TagEnd::Item) => {
                    self.flush_line();
                }

                Event::Start(Tag::BlockQuote(_)) => {
                    self.blockquote_depth += 1;
                    let bq_style = self.current_style().fg(self.theme.blockquote_fg);
                    self.style_stack.push(bq_style);
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    self.blockquote_depth = self.blockquote_depth.saturating_sub(1);
                    self.pop_modifier();
                }

                // -- Table rendering -----------------------------------------
                Event::Start(Tag::Table(alignments)) => {
                    self.table_alignments = alignments;
                    self.table_rows.clear();
                }
                Event::End(TagEnd::Table) => {
                    self.render_table();
                    self.table_rows.clear();
                    self.table_alignments.clear();
                }
                Event::Start(Tag::TableHead | Tag::TableRow) => {
                    self.table_row.clear();
                }
                Event::End(TagEnd::TableHead | TagEnd::TableRow) => {
                    self.table_rows.push(std::mem::take(&mut self.table_row));
                }
                Event::Start(Tag::TableCell) => {
                    self.current_spans.clear();
                }
                Event::End(TagEnd::TableCell) => {
                    let spans = std::mem::take(&mut self.current_spans);
                    let display_width: usize = spans
                        .iter()
                        .map(|s| UnicodeWidthStr::width(s.content.as_ref()))
                        .sum();
                    self.table_row.push(TableCell {
                        spans,
                        display_width,
                    });
                }

                // -- Inline tags ---------------------------------------------
                Event::Start(Tag::Emphasis) => {
                    self.push_modifier(Modifier::ITALIC);
                }
                Event::End(TagEnd::Emphasis) => {
                    self.pop_modifier();
                }

                Event::Start(Tag::Strong) => {
                    self.push_modifier(Modifier::BOLD);
                }
                Event::End(TagEnd::Strong) => {
                    self.pop_modifier();
                }

                Event::Start(Tag::Strikethrough) => {
                    self.push_modifier(Modifier::CROSSED_OUT);
                }
                Event::End(TagEnd::Strikethrough) => {
                    self.pop_modifier();
                }

                Event::Start(Tag::Link { .. }) => {
                    let link_style = self.current_style()
                        .fg(self.theme.link_fg)
                        .add_modifier(Modifier::UNDERLINED);
                    self.style_stack.push(link_style);
                }
                Event::End(TagEnd::Link) => {
                    self.pop_modifier();
                }

                // -- Leaf events ---------------------------------------------
                Event::Code(text) => {
                    let style = Style::default()
                        .fg(self.theme.inline_code_fg)
                        .bg(self.theme.inline_code_bg);
                    self.current_spans
                        .push(Span::styled(format!("`{}`", text), style));
                }

                Event::Text(text) => {
                    if self.code_block.is_some() {
                        self.code_text.push_str(&text);
                    } else {
                        // Table cell text goes through current_spans like
                        // normal inline text so that formatting is preserved.
                        self.current_spans
                            .push(Span::styled(text.to_string(), self.current_style()));
                    }
                }

                Event::SoftBreak => {
                    self.current_spans.push(Span::raw(" "));
                }
                Event::HardBreak => {
                    self.flush_line();
                }

                Event::Rule => {
                    self.flush_line();
                    let rule = "─".repeat(40);
                    self.lines.push(Line::from(Span::styled(
                        rule,
                        Style::default().fg(self.theme.text_hr),
                    )));
                    self.lines.push(Line::from(""));
                }

                Event::TaskListMarker(checked) => {
                    let marker = if checked { "[x] " } else { "[ ] " };
                    self.current_spans
                        .push(Span::styled(marker.to_string(), self.current_style()));
                }

                // Ignore events we don't handle yet.
                _ => {}
            }
        }

        // Flush any remaining spans.
        if !self.current_spans.is_empty() {
            self.flush_line();
        }
    }

    /// Flush accumulated spans into a completed [`Line`] and push it.
    fn flush_line(&mut self) {
        if self.current_spans.is_empty() && self.blockquote_depth == 0 {
            return;
        }

        let mut spans = std::mem::take(&mut self.current_spans);

        if self.blockquote_depth > 0 {
            let prefix_style = Style::default()
                .fg(self.theme.blockquote_border);
            let prefix = "\u{2502} ".repeat(self.blockquote_depth);
            spans.insert(0, Span::styled(prefix, prefix_style));
        }

        self.lines.push(Line::from(spans));
    }

    /// Render a completed code block with syntax highlighting.
    fn render_code_block(&mut self) {
        let lang = self.code_block.as_deref().unwrap_or("");
        let ss = syntax_set();
        let ts = theme_set();

        let syntect_theme = ts
            .themes
            .get(self.theme.syntect_theme)
            .or_else(|| ts.themes.values().next())
            .expect("syntect has at least one bundled theme");

        // Find syntax for the language, fall back to plain text
        let syntax = if lang.is_empty() {
            ss.find_syntax_plain_text()
        } else {
            ss.find_syntax_by_token(lang)
                .unwrap_or_else(|| ss.find_syntax_plain_text())
        };

        let bg = self.theme.code_block_bg;
        let border_style = Style::default().fg(self.theme.code_block_border);
        let label_style = Style::default().fg(self.theme.code_block_lang_label);

        // Top border with language label
        if lang.is_empty() {
            let top_border = format!("  {}", "\u{2500}".repeat(CODE_BORDER_WIDTH));
            self.lines
                .push(Line::from(Span::styled(top_border, border_style)));
        } else {
            let lang_label = format!(" {} ", lang);
            let remaining = (CODE_BORDER_WIDTH - CODE_LABEL_PREFIX_WIDTH)
                .saturating_sub(lang_label.len());
            let top_line = vec![
                Span::styled(
                    format!("  {}", "\u{2500}".repeat(CODE_LABEL_PREFIX_WIDTH)),
                    border_style,
                ),
                Span::styled(lang_label, label_style),
                Span::styled("\u{2500}".repeat(remaining), border_style),
            ];
            self.lines.push(Line::from(top_line));
        }

        // Highlighted code lines
        let mut highlighter = HighlightLines::new(syntax, syntect_theme);
        for code_line in self.code_text.lines() {
            let regions = highlighter
                .highlight_line(code_line, ss)
                .unwrap_or_default();

            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                "    ".to_string(),
                Style::default().bg(bg),
            )];
            for (style, text) in regions {
                spans.push(Span::styled(
                    text.to_string(),
                    translate_style(style).bg(bg),
                ));
            }
            self.lines.push(Line::from(spans));
        }

        // Bottom border
        let bottom_border = format!("  {}", "\u{2500}".repeat(CODE_BORDER_WIDTH));
        self.lines
            .push(Line::from(Span::styled(bottom_border, border_style)));
    }

    /// Render accumulated table data with aligned columns and box-drawing borders.
    fn render_table(&mut self) {
        if self.table_rows.is_empty() {
            return;
        }

        let border_style = Style::default().fg(self.theme.table_border);
        let header_style = Style::default()
            .fg(self.theme.table_header_fg)
            .bg(self.theme.table_header_bg)
            .add_modifier(Modifier::BOLD);

        // Calculate column count and widths using Unicode display width.
        let col_count = self.table_rows.iter().map(|r| r.len()).max().unwrap_or(0);
        if col_count == 0 {
            return;
        }

        let mut col_widths: Vec<usize> = vec![0; col_count];
        for row in &self.table_rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_count {
                    col_widths[i] = col_widths[i].max(cell.display_width);
                }
            }
        }

        // Ensure minimum column width of 3.
        for w in &mut col_widths {
            *w = (*w).max(3);
        }

        // Helper to build a horizontal border line.
        let build_border =
            |left: &str, mid: &str, right: &str, fill: &str| -> Line<'static> {
                let mut s = String::from("  ");
                s.push_str(left);
                for (i, &w) in col_widths.iter().enumerate() {
                    s.push_str(&fill.repeat(w + 2));
                    if i < col_count - 1 {
                        s.push_str(mid);
                    }
                }
                s.push_str(right);
                Line::from(Span::styled(s, border_style))
            };

        // Helper to build a data row with column alignment and styled cell spans.
        let alignments = &self.table_alignments;
        let build_row = |row: &[TableCell], extra_style: Style| -> Line<'static> {
            let mut spans: Vec<Span<'static>> = vec![Span::styled(
                "  \u{2502}",
                border_style,
            )];
            for (i, w) in col_widths.iter().enumerate() {
                let cell = row.get(i);
                let cell_width = cell.map_or(0, |c| c.display_width);
                let alignment = alignments.get(i).copied().unwrap_or(pulldown_cmark::Alignment::None);

                let total_pad = w.saturating_sub(cell_width);
                let (left_pad, right_pad) = match alignment {
                    pulldown_cmark::Alignment::Center => {
                        let lp = total_pad / 2;
                        (lp, total_pad - lp)
                    }
                    pulldown_cmark::Alignment::Right => (total_pad, 0),
                    _ => (0, total_pad),
                };

                // Left padding + space
                spans.push(Span::styled(
                    format!(" {}", " ".repeat(left_pad)),
                    extra_style,
                ));
                // Cell content spans with extra_style merged
                if let Some(cell) = cell {
                    for span in &cell.spans {
                        spans.push(Span::styled(
                            span.content.to_string(),
                            span.style.patch(extra_style),
                        ));
                    }
                }
                // Right padding + space
                spans.push(Span::styled(
                    format!("{} ", " ".repeat(right_pad)),
                    extra_style,
                ));
                if i < col_count - 1 {
                    spans.push(Span::styled("\u{2502}", border_style));
                }
            }
            spans.push(Span::styled("\u{2502}", border_style));
            Line::from(spans)
        };

        // Top border: ┌───┬───┐
        self.lines
            .push(build_border("\u{250c}", "\u{252c}", "\u{2510}", "\u{2500}"));

        // Header row (first row).
        if let Some(header) = self.table_rows.first() {
            self.lines.push(build_row(header, header_style));
            // Header separator: ├───┼───┤
            self.lines
                .push(build_border("\u{251c}", "\u{253c}", "\u{2524}", "\u{2500}"));
        }

        // Data rows.
        let cell_style = Style::default();
        for row in self.table_rows.iter().skip(1) {
            self.lines.push(build_row(row, cell_style));
        }

        // Bottom border: └───┴───┘
        self.lines
            .push(build_border("\u{2514}", "\u{2534}", "\u{2518}", "\u{2500}"));
    }

    /// Return the currently active style from the top of the stack.
    fn current_style(&self) -> Style {
        self.style_stack
            .last()
            .copied()
            .unwrap_or_default()
    }

    /// Push a new style with the given modifier added to the current style.
    fn push_modifier(&mut self, modifier: Modifier) {
        let current = self.current_style();
        self.style_stack.push(current.add_modifier(modifier));
    }

    /// Pop the most recently pushed modifier style.
    fn pop_modifier(&mut self) {
        // Never pop the base style.
        if self.style_stack.len() > 1 {
            self.style_stack.pop();
        }
    }
}
