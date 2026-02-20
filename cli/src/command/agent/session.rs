//! Session discovery and JSONL parsing for Claude Code session files.
//!
//! Provides functions to discover previous Claude Code sessions on disk
//! and parse their JSONL content for display in the session picker.

use std::fs;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::state::{compute_line_diff, DisplayLine};

/// Metadata for a single Claude Code session discovered on disk.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// The session UUID (filename stem of the .jsonl file).
    pub session_id: String,
    /// Human-readable slug (e.g. "cryptic-growing-horizon").
    pub slug: Option<String>,
    /// Timestamp of the last message in the session.
    #[allow(dead_code)]
    pub last_timestamp: String,
    /// Preview of the last meaningful message (user or assistant text).
    /// Truncated to ~200 chars for display.
    pub last_message_preview: String,
    /// Role of the last message ("user" or "assistant").
    pub last_message_role: String,
    /// Full path to the .jsonl file on disk.
    pub file_path: PathBuf,
    /// File modification time (for sorting).
    pub modified: SystemTime,
}

/// Convert a workspace path to the Claude Code projects directory path.
///
/// Claude Code maps workspace paths by replacing `/` and `.` with `-`.
/// The leading `/` naturally produces the `-` prefix after replacement.
/// For example: `/Users/erik/my.project` becomes `~/.claude/projects/-Users-erik-my-project/`
pub fn compute_session_dir(workspace: &Path) -> PathBuf {
    let path_str = workspace.to_string_lossy();
    let dir_name = path_str.replace(['/', '.'], "-");
    let home = std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("~"));
    home.join(".claude").join("projects").join(dir_name)
}

/// Discover all Claude Code sessions for the given workspace.
///
/// Lists all `*.jsonl` files in the session directory, extracts preview
/// metadata from each, and returns them sorted by modification time
/// (most recent first).
pub fn discover_sessions(workspace: &Path) -> Vec<SessionInfo> {
    let session_dir = compute_session_dir(workspace);
    let entries = match fs::read_dir(&session_dir) {
        Ok(entries) => entries,
        Err(_) => return Vec::new(),
    };

    let mut sessions: Vec<SessionInfo> = entries
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                parse_session_preview(&path)
            } else {
                None
            }
        })
        .collect();

    // Sort by modification time, most recent first.
    sessions.sort_by(|a, b| b.modified.cmp(&a.modified));
    sessions
}

/// Parse preview metadata from a single JSONL session file.
///
/// Reads the head of the file (~10KB) to find the first user prompt,
/// and the tail (~4KB) to grab the session slug.
pub fn parse_session_preview(file_path: &Path) -> Option<SessionInfo> {
    let file = fs::File::open(file_path).ok()?;
    let metadata = file.metadata().ok()?;
    let modified = metadata.modified().ok()?;
    let file_size = metadata.len();

    let session_id = file_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
        .to_string();

    // Read the head of the file (first 10KB) to find the first user prompt.
    let head_size = file_size.min(10_240) as usize;
    let mut reader = BufReader::new(&file);
    let mut head_buf = vec![0u8; head_size];
    reader.read_exact(&mut head_buf).ok()?;
    let head_content = String::from_utf8_lossy(&head_buf);

    let mut last_timestamp = String::new();
    let mut last_message_preview = String::new();
    let mut last_message_role = String::new();
    let mut found = false;

    for line in head_content.lines() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");
        if msg_type != "user" {
            continue;
        }

        let message = match val.get("message") {
            Some(m) => m,
            None => continue,
        };

        let content = message.get("content");
        let preview = match content {
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(serde_json::Value::Array(arr)) => {
                arr.iter()
                    .find_map(|block| {
                        if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                            block.get("text").and_then(|t| t.as_str()).map(String::from)
                        } else {
                            None
                        }
                    })
                    .unwrap_or_default()
            }
            _ => continue,
        };

        if preview.is_empty() {
            continue;
        }

        last_timestamp = val
            .get("timestamp")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        last_message_role = "user".to_string();
        last_message_preview = if preview.len() > 200 {
            let mut end = 197;
            while !preview.is_char_boundary(end) {
                end -= 1;
            }
            format!("{}...", &preview[..end])
        } else {
            preview
        };
        last_message_preview = last_message_preview.replace('\n', " ");
        found = true;
        break;
    }

    // Read the tail of the file (~4KB) to grab the session slug.
    let slug = if file_size > 4_096 {
        let mut tail_reader = BufReader::new(fs::File::open(file_path).ok()?);
        tail_reader.seek(SeekFrom::End(-4_096)).ok()?;
        let mut buf = String::new();
        tail_reader.read_to_string(&mut buf).ok()?;
        buf.lines().rev().find_map(|line| {
            let val: serde_json::Value = serde_json::from_str(line).ok()?;
            let s = val.get("slug")?.as_str()?;
            if s.is_empty() { None } else { Some(s.to_string()) }
        })
    } else {
        head_content.lines().rev().find_map(|line| {
            let val: serde_json::Value = serde_json::from_str(line).ok()?;
            let s = val.get("slug")?.as_str()?;
            if s.is_empty() { None } else { Some(s.to_string()) }
        })
    };

    if !found {
        return Some(SessionInfo {
            session_id,
            slug,
            last_timestamp: String::new(),
            last_message_preview: "(empty session)".to_string(),
            last_message_role: String::new(),
            file_path: file_path.to_path_buf(),
            modified,
        });
    }

    Some(SessionInfo {
        session_id,
        slug,
        last_timestamp,
        last_message_preview,
        last_message_role,
        file_path: file_path.to_path_buf(),
        modified,
    })
}

/// Load a full session conversation from a JSONL file for display.
///
/// Reads the entire file and maps each JSON line to the appropriate
/// `DisplayLine` variant. Lines that don't represent displayable content
/// (system, progress, file-history-snapshot) are skipped.
pub fn load_session_conversation(file_path: &Path) -> Vec<DisplayLine> {
    let content = match fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let mut lines = Vec::new();
    // Tracks the last Edit tool's (old_string, new_string, file_path) for diff generation.
    let mut last_edit_input: Option<(String, String, String)> = None;

    for line in content.lines() {
        let val: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        let msg_type = val.get("type").and_then(|v| v.as_str()).unwrap_or("");

        match msg_type {
            "user" => {
                if let Some(message) = val.get("message") {
                    if let Some(content_arr) = message.get("content").and_then(|c| c.as_array()) {
                        // Array content: may contain text blocks (user prompt) or
                        // tool_result blocks (tool output).
                        let mut has_tool_results = false;
                        for block in content_arr {
                            let block_type =
                                block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            match block_type {
                                "text" => {
                                    if let Some(text) = block.get("text").and_then(|t| t.as_str())
                                    {
                                        lines.push(DisplayLine::UserPrompt {
                                            content: text.to_string(),
                                        });
                                    }
                                }
                                "tool_result" => {
                                    has_tool_results = true;
                                    let is_error = block
                                        .get("is_error")
                                        .and_then(|e| e.as_bool())
                                        .unwrap_or(false);

                                    // Emit DiffResult if the preceding tool was an Edit.
                                    if !is_error {
                                        if let Some((old_s, new_s, file_path)) =
                                            last_edit_input.take()
                                        {
                                            let diff_ops = compute_line_diff(&old_s, &new_s);
                                            lines.push(DisplayLine::DiffResult {
                                                diff_ops,
                                                file_path,
                                            });
                                        }
                                    } else {
                                        last_edit_input = None;
                                    }

                                    let result_content = extract_tool_result_text(
                                        block.get("content").unwrap_or(&serde_json::Value::Null),
                                    );
                                    if !result_content.is_empty() {
                                        lines.push(DisplayLine::ToolResult {
                                            content: result_content,
                                            is_error,
                                        });
                                    }
                                }
                                _ => {}
                            }
                        }
                        // If no tool_result blocks were found, any pending Edit
                        // input is stale — clear it.
                        if !has_tool_results {
                            last_edit_input = None;
                        }
                    } else if let Some(text) =
                        message.get("content").and_then(|c| c.as_str())
                    {
                        // String content: direct user prompt.
                        lines.push(DisplayLine::UserPrompt {
                            content: text.to_string(),
                        });
                    }
                }
            }
            "assistant" => {
                if let Some(message) = val.get("message") {
                    if let Some(content_arr) = message.get("content").and_then(|c| c.as_array()) {
                        for block in content_arr {
                            let block_type =
                                block.get("type").and_then(|t| t.as_str()).unwrap_or("");
                            match block_type {
                                "text" => {
                                    if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                                        lines.push(DisplayLine::Text(text.to_string()));
                                    }
                                }
                                "tool_use" => {
                                    let tool = block
                                        .get("name")
                                        .and_then(|n| n.as_str())
                                        .unwrap_or("unknown")
                                        .to_string();
                                    let input_val = block.get("input");
                                    let input_preview = input_val
                                        .map(|i| {
                                            let s = i.to_string();
                                            if s.len() > 100 {
                                                let mut end = 97;
                                                while !s.is_char_boundary(end) {
                                                    end -= 1;
                                                }
                                                format!("{}...", &s[..end])
                                            } else {
                                                s
                                            }
                                        })
                                        .unwrap_or_default();

                                    // Capture Edit tool input for diff generation.
                                    if tool == "Edit" {
                                        last_edit_input = input_val.and_then(|v| {
                                            let fp = v
                                                .get("file_path")?
                                                .as_str()?
                                                .to_string();
                                            let old = v
                                                .get("old_string")?
                                                .as_str()?
                                                .to_string();
                                            let new = v
                                                .get("new_string")?
                                                .as_str()?
                                                .to_string();
                                            Some((old, new, fp))
                                        });
                                    } else {
                                        last_edit_input = None;
                                    }

                                    lines.push(DisplayLine::ToolUse {
                                        tool,
                                        input_preview,
                                    });
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }

    lines
}

/// Extract text content from a tool result's `content` field.
///
/// Handles string values, arrays of `{type: "text", text: "..."}` blocks,
/// and null/missing content.
fn extract_tool_result_text(content: &serde_json::Value) -> String {
    match content {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Array(arr) => {
            let mut texts = Vec::new();
            for block in arr {
                if let Some(text) = block.get("text").and_then(|t| t.as_str()) {
                    texts.push(text.to_string());
                }
            }
            texts.join("\n")
        }
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}
