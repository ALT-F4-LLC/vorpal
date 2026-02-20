//! Session export to Markdown for sharing and documentation.
//!
//! Provides [`export_session`] which writes the focused agent's session
//! (configuration, prompt, and all output) to a Markdown file in the
//! agent's workspace directory.

use super::state::{AgentState, AgentStatus, DiffLine, DisplayLine};
use super::ui::{BLOCK_MARKER, RESULT_CONNECTOR, SESSION_MARKER};
use std::fmt::Write;
use std::path::PathBuf;

/// Escape pipe characters in a string for use inside Markdown table cells.
fn escape_table_cell(s: &str) -> String {
    s.replace('|', "\\|")
}

/// Choose a code fence that does not collide with content.
///
/// Returns `` ``` ``, `` ```` ``, or `` ````` `` depending on what the
/// content already contains.
fn safe_fence(content: &str) -> &'static str {
    if content.contains("````") {
        "`````"
    } else if content.contains("```") {
        "````"
    } else {
        "```"
    }
}

/// Export an agent's session to a Markdown file.
///
/// Returns `Ok(path)` with the path to the written file on success, or an
/// error message on failure.
pub fn export_session(agent: &AgentState) -> Result<PathBuf, String> {
    let mut md = String::new();

    // -- Header -----------------------------------------------------------
    writeln!(md, "# Agent Session Export").unwrap();
    writeln!(md).unwrap();

    // -- Configuration section --------------------------------------------
    writeln!(md, "## Configuration").unwrap();
    writeln!(md).unwrap();
    writeln!(md, "| Setting | Value |",).unwrap();
    writeln!(md, "|---------|-------|").unwrap();
    writeln!(
        md,
        "| Workspace | `{}` |",
        escape_table_cell(&agent.workspace.display().to_string())
    )
    .unwrap();

    let status_str = match &agent.status {
        AgentStatus::Running => "Running".to_string(),
        AgentStatus::Exited(Some(code)) => format!("Exited (code {code})"),
        AgentStatus::Exited(None) => "Exited".to_string(),
    };
    writeln!(md, "| Status | {} |", escape_table_cell(&status_str)).unwrap();

    if let Some(ref session_id) = agent.session_id {
        writeln!(md, "| Session ID | `{}` |", escape_table_cell(session_id)).unwrap();
    }

    let opts = &agent.claude_options;
    if let Some(ref mode) = opts.permission_mode {
        writeln!(md, "| Permission Mode | `{}` |", escape_table_cell(mode)).unwrap();
    }
    if let Some(ref model) = opts.model {
        writeln!(md, "| Model | `{}` |", escape_table_cell(model)).unwrap();
    }
    if let Some(ref effort) = opts.effort {
        writeln!(md, "| Effort | `{}` |", escape_table_cell(effort)).unwrap();
    }
    if let Some(budget) = opts.max_budget_usd {
        writeln!(md, "| Max Budget | ${budget:.2} |").unwrap();
    }
    if !opts.allowed_tools.is_empty() {
        writeln!(
            md,
            "| Allowed Tools | `{}` |",
            escape_table_cell(&opts.allowed_tools.join(", "))
        )
        .unwrap();
    }
    if !opts.add_dirs.is_empty() {
        writeln!(
            md,
            "| Additional Dirs | `{}` |",
            escape_table_cell(&opts.add_dirs.join(", "))
        )
        .unwrap();
    }

    writeln!(md).unwrap();

    // -- Prompt section ----------------------------------------------------
    writeln!(md, "## Prompt").unwrap();
    writeln!(md).unwrap();
    let prompt_fence = safe_fence(&agent.prompt);
    writeln!(md, "{prompt_fence}").unwrap();
    writeln!(md, "{}", agent.prompt).unwrap();
    writeln!(md, "{prompt_fence}").unwrap();
    writeln!(md).unwrap();

    // -- Output section ----------------------------------------------------
    writeln!(md, "## Output").unwrap();
    writeln!(md).unwrap();

    if agent.output.is_empty() {
        writeln!(md, "_No output recorded._").unwrap();
    } else {
        let mut in_tool_result = false;

        for line in &agent.output {
            match line {
                DisplayLine::Text(s) => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md, "{s}").unwrap();
                }
                DisplayLine::Thinking(s) => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md, "> {BLOCK_MARKER} {s}").unwrap();
                }
                DisplayLine::ToolUse {
                    tool,
                    input_preview,
                } => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md).unwrap();
                    writeln!(md, "### {BLOCK_MARKER} `{tool}`").unwrap();
                    writeln!(md).unwrap();
                    if !input_preview.is_empty() {
                        writeln!(md, "**Input:** `{input_preview}`").unwrap();
                        writeln!(md).unwrap();
                    }
                }
                DisplayLine::ToolResult { content, is_error } => {
                    if !in_tool_result {
                        if *is_error {
                            writeln!(md, "**Result (ERROR):**").unwrap();
                        } else {
                            writeln!(md, "**Result:**").unwrap();
                        }
                        writeln!(md, "````").unwrap();
                        in_tool_result = true;
                    }
                    writeln!(md, "{content}").unwrap();
                }
                DisplayLine::Result(s) => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md).unwrap();
                    writeln!(md, "---").unwrap();
                    writeln!(md, "{SESSION_MARKER} {s}").unwrap();
                }
                DisplayLine::System(s) => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md, "_{RESULT_CONNECTOR} {s}_").unwrap();
                }
                DisplayLine::Stderr(s) => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md, "_{s}_").unwrap();
                }
                DisplayLine::Error(s) => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md, "**ERROR:** {s}").unwrap();
                }
                DisplayLine::DiffResult {
                    diff_ops,
                    file_path,
                } => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md, "**Diff** `{file_path}`:").unwrap();
                    writeln!(md, "```diff").unwrap();
                    for op in diff_ops {
                        match op {
                            DiffLine::Equal(line) => writeln!(md, "  {line}").unwrap(),
                            DiffLine::Delete(line) => writeln!(md, "- {line}").unwrap(),
                            DiffLine::Insert(line) => writeln!(md, "+ {line}").unwrap(),
                        }
                    }
                    writeln!(md, "```").unwrap();
                    writeln!(md).unwrap();
                }
                DisplayLine::AgentMessage {
                    sender,
                    recipient,
                    content,
                } => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md, "> **{sender}** -> **{recipient}**: {content}").unwrap();
                }
                DisplayLine::TurnSummary {
                    input_tokens,
                    output_tokens,
                    cost_usd,
                } => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md).unwrap();
                    writeln!(
                        md,
                        "*{} in / {} out | ${:.2}*",
                        input_tokens, output_tokens, cost_usd
                    )
                    .unwrap();
                }
                DisplayLine::TurnStart { .. } => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md).unwrap();
                    writeln!(md, "---").unwrap();
                    writeln!(md).unwrap();
                }
                DisplayLine::UserPrompt { content } => {
                    if in_tool_result {
                        writeln!(md, "````").unwrap();
                        writeln!(md).unwrap();
                        in_tool_result = false;
                    }
                    writeln!(md).unwrap();
                    for (idx, line) in content.lines().enumerate() {
                        if idx == 0 {
                            writeln!(md, "> **You:** {line}").unwrap();
                        } else {
                            writeln!(md, "> {line}").unwrap();
                        }
                    }
                    // Handle empty content.
                    if content.is_empty() {
                        writeln!(md, "> **You:**").unwrap();
                    }
                    writeln!(md).unwrap();
                }
            }
        }

        // Close any trailing code block.
        if in_tool_result {
            writeln!(md, "````").unwrap();
        }
    }

    // -- Generate filename and write to disk --------------------------------
    // Write to the agent's workspace directory so the export lands next to
    // the code the agent was working on, rather than wherever the TUI
    // process was launched from.
    let (date_str, time_str) = format_date_time_today();
    let filename = format!("agent-{}-{date_str}-{time_str}.md", agent.id + 1);
    let path = agent.workspace.join(&filename);

    std::fs::write(&path, md).map_err(|e| format!("Failed to write {filename}: {e}"))?;

    Ok(path)
}

/// Format the current UTC date and time as `(YYYY-MM-DD, HHMMSS)` using only std.
///
/// Both components use UTC to ensure deterministic, timezone-independent filenames.
fn format_date_time_today() -> (String, String) {
    use std::time::{SystemTime, UNIX_EPOCH};

    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    // Time-of-day components.
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since epoch.
    let days = (secs / 86400) as i64;

    // Civil date from day count (algorithm from Howard Hinnant).
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    (
        format!("{y:04}-{m:02}-{d:02}"),
        format!("{hours:02}{minutes:02}{seconds:02}"),
    )
}
