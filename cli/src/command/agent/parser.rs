//! Claude Code stream-json output parser.
//!
//! Deserializes Claude Code's `--output-format stream-json` newline-delimited
//! JSON output into structured [`DisplayLine`](super::state::DisplayLine) values
//! for rendering in the TUI.

use super::state::{compute_line_diff, DisplayLine};
use serde::Deserialize;
use tracing::debug;

// ---------------------------------------------------------------------------
// Serde types for Claude Code stream-json protocol
// ---------------------------------------------------------------------------

/// Top-level JSON line emitted by Claude Code with `--output-format stream-json`.
///
/// Uses `#[serde(untagged)]` to allow a catch-all fallback for unknown event
/// types. Variants are tried in order: the internally-tagged variants first,
/// then `Unknown` catches anything else.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StreamLine {
    /// One of the known, internally-tagged event types.
    Known(KnownStreamLine),
    /// Catch-all for unrecognised event types — silently ignored.
    #[allow(dead_code)]
    Unknown(serde_json::Value),
}

/// Known event types emitted by Claude Code.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum KnownStreamLine {
    /// System/hook events (e.g. hook_started, hook_response).
    #[serde(rename = "system")]
    System {
        subtype: String,
        #[serde(flatten)]
        extra: serde_json::Value,
    },
    /// Wrapped Anthropic streaming API events.
    #[serde(rename = "stream_event")]
    StreamEvent { event: ApiEvent },
    /// Complete assistant response (emitted after all stream events).
    #[serde(rename = "assistant")]
    Assistant {
        #[allow(dead_code)]
        message: AssistantMessage,
    },
    /// User message containing tool results.
    #[serde(rename = "user")]
    User { message: UserMessage },
    /// Final result summary with cost and session info.
    #[serde(rename = "result")]
    Result {
        #[allow(dead_code)]
        result: String,
        #[serde(default)]
        subtype: Option<String>,
        #[serde(default)]
        total_cost_usd: Option<f64>,
        #[serde(default)]
        session_id: Option<String>,
    },
}

/// The `message` payload inside an `assistant` event.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AssistantMessage {
    #[serde(default)]
    content: Vec<AssistantContentBlock>,
}

/// A single content block inside an assistant message.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(dead_code)]
enum AssistantContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse {
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
}

/// The `message` payload inside a `user` event.
#[derive(Debug, Deserialize)]
struct UserMessage {
    #[serde(default)]
    content: Vec<UserContentBlock>,
}

/// A single content block inside a user message.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum UserContentBlock {
    #[serde(rename = "tool_result")]
    ToolResult {
        #[allow(dead_code)]
        tool_use_id: String,
        #[serde(default)]
        content: serde_json::Value,
        #[serde(default)]
        is_error: bool,
    },
    /// Catch-all for unrecognised user content block types.
    #[serde(other)]
    Unknown,
}

/// Anthropic streaming API event, nested inside a `stream_event` line.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ApiEvent {
    #[serde(rename = "message_start")]
    MessageStart {
        #[allow(dead_code)]
        message: serde_json::Value,
    },
    #[serde(rename = "content_block_start")]
    ContentBlockStart {
        #[allow(dead_code)]
        index: usize,
        content_block: ContentBlock,
    },
    #[serde(rename = "content_block_delta")]
    ContentBlockDelta {
        #[allow(dead_code)]
        index: usize,
        delta: Delta,
    },
    #[serde(rename = "content_block_stop")]
    ContentBlockStop {
        #[allow(dead_code)]
        index: usize,
    },
    #[serde(rename = "message_delta")]
    MessageDelta {
        #[allow(dead_code)]
        delta: serde_json::Value,
        #[serde(default)]
        #[allow(dead_code)]
        usage: serde_json::Value,
    },
    #[serde(rename = "message_stop")]
    MessageStop,
}

/// Content block type announced by `content_block_start`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
enum ContentBlock {
    #[serde(rename = "text")]
    Text {
        #[allow(dead_code)]
        text: String,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[allow(dead_code)]
        id: String,
        name: String,
        #[serde(default)]
        #[allow(dead_code)]
        input: serde_json::Value,
    },
    #[serde(rename = "thinking")]
    Thinking {
        #[allow(dead_code)]
        thinking: String,
    },
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        #[allow(dead_code)]
        id: String,
        name: String,
        #[serde(default)]
        input: serde_json::Value,
    },
    #[serde(rename = "server_tool_result")]
    ServerToolResult {
        #[allow(dead_code)]
        tool_use_id: String,
        #[serde(default)]
        content: serde_json::Value,
    },
}

/// Delta payload inside a `content_block_delta`.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
#[allow(clippy::enum_variant_names)] // names match the Claude API protocol
enum Delta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
    #[serde(rename = "thinking_delta")]
    ThinkingDelta { thinking: String },
}

// ---------------------------------------------------------------------------
// Internal tracking for which kind of content block is active
// ---------------------------------------------------------------------------

/// Tracks the type of the currently open content block so we know what to
/// flush when `content_block_stop` arrives.
#[derive(Debug, Clone, PartialEq, Eq)]
enum ActiveBlock {
    Text,
    ToolUse,
    Thinking,
}

// ---------------------------------------------------------------------------
// Parser
// ---------------------------------------------------------------------------

/// Maximum length of the tool input preview included in [`DisplayLine::ToolUse`].
const TOOL_INPUT_PREVIEW_MAX: usize = 100;

/// Stateful parser that converts Claude Code stream-json lines into
/// [`DisplayLine`] values.
///
/// The parser accumulates text deltas and tool input deltas across multiple
/// calls to [`parse_line`](Self::parse_line), flushing them as complete
/// display lines when newlines are encountered or content blocks end.
#[derive(Default)]
pub struct Parser {
    /// Accumulates `text_delta` chunks. Flushed on newlines or block stop.
    text_buffer: String,
    /// Accumulates `thinking_delta` chunks. Flushed on newlines or block stop.
    thinking_buffer: String,
    /// The tool name for the currently active `tool_use` content block.
    current_tool: Option<String>,
    /// Accumulates `input_json_delta` chunks for the current tool_use block.
    tool_input_buffer: String,
    /// Tracks whether the current content block is text, tool_use, or thinking.
    active_block: Option<ActiveBlock>,
    /// The last session ID seen in a `result` event, ready to be taken by the caller.
    last_session_id: Option<String>,
    /// The last completed Edit tool's input parameters, used to generate
    /// [`DisplayLine::DiffResult`] when the corresponding tool result arrives.
    last_edit_input: Option<EditInput>,
}

/// Captured input parameters from an Edit tool call.
#[derive(Debug, Clone, Default)]
struct EditInput {
    file_path: String,
    old_string: String,
    new_string: String,
}

impl Parser {
    /// Create a new parser with empty accumulation state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Take the last session ID seen in a `result` event, if any.
    ///
    /// Returns `Some(session_id)` exactly once per result event — subsequent
    /// calls return `None` until another result event is parsed.
    pub fn take_session_id(&mut self) -> Option<String> {
        self.last_session_id.take()
    }

    /// Parse a single JSON line and return zero or more [`DisplayLine`] values.
    ///
    /// Blank lines are silently ignored. Malformed JSON produces a single
    /// [`DisplayLine::Error`] (the parser never panics on bad input).
    pub fn parse_line(&mut self, line: &str) -> Vec<DisplayLine> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Vec::new();
        }

        match serde_json::from_str::<StreamLine>(trimmed) {
            Ok(stream_line) => self.handle_stream_line(stream_line),
            Err(e) => {
                debug!(error = %e, line, "malformed JSON line from agent stream");
                vec![DisplayLine::Error(line.to_string())]
            }
        }
    }

    // -- internal dispatch --------------------------------------------------

    fn handle_stream_line(&mut self, line: StreamLine) -> Vec<DisplayLine> {
        match line {
            StreamLine::Known(known) => self.handle_known_line(known),
            StreamLine::Unknown(value) => {
                let event_type = value
                    .get("type")
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown");
                debug!(event_type, "unhandled stream-json event type");
                Vec::new()
            }
        }
    }

    fn handle_known_line(&mut self, line: KnownStreamLine) -> Vec<DisplayLine> {
        match line {
            KnownStreamLine::System { subtype, extra } => {
                vec![DisplayLine::System(format_system_event(&subtype, &extra))]
            }
            KnownStreamLine::StreamEvent { event } => self.handle_api_event(event),
            // Content already streamed via stream_event deltas; the assistant
            // event is a duplicate summary — skip it to avoid double output.
            KnownStreamLine::Assistant { .. } => Vec::new(),
            KnownStreamLine::User { message } => self.handle_user_message(message),
            KnownStreamLine::Result {
                subtype,
                total_cost_usd,
                session_id,
                ..
            } => {
                self.last_session_id = session_id.clone();
                let mut parts = Vec::new();
                if let Some(sub) = subtype {
                    parts.push(format!("◈ Status: {sub}"));
                }
                if let Some(cost) = total_cost_usd {
                    parts.push(format!("◇ Cost: ${cost:.4}"));
                }
                if let Some(sid) = session_id {
                    parts.push(format!("⊙ Session: {sid}"));
                }
                if parts.is_empty() {
                    Vec::new()
                } else {
                    vec![DisplayLine::Result(parts.join(" | "))]
                }
            }
        }
    }

    fn handle_api_event(&mut self, event: ApiEvent) -> Vec<DisplayLine> {
        match event {
            ApiEvent::MessageStart { .. } => vec![DisplayLine::TurnStart],
            ApiEvent::MessageStop => Vec::new(),

            ApiEvent::MessageDelta { .. } => Vec::new(),

            ApiEvent::ContentBlockStart { content_block, .. } => {
                self.handle_content_block_start(content_block)
            }

            ApiEvent::ContentBlockDelta { delta, .. } => self.handle_delta(delta),

            ApiEvent::ContentBlockStop { .. } => self.handle_content_block_stop(),
        }
    }

    fn handle_content_block_start(&mut self, block: ContentBlock) -> Vec<DisplayLine> {
        match block {
            ContentBlock::Text { .. } => {
                self.active_block = Some(ActiveBlock::Text);
                Vec::new()
            }
            ContentBlock::ToolUse { name, .. } => {
                self.active_block = Some(ActiveBlock::ToolUse);
                self.current_tool = Some(name);
                self.tool_input_buffer.clear();
                Vec::new()
            }
            ContentBlock::Thinking { .. } => {
                self.active_block = Some(ActiveBlock::Thinking);
                self.thinking_buffer.clear();
                Vec::new()
            }
            ContentBlock::ServerToolUse { name, input, .. } => {
                // Server tool use arrives complete — no delta accumulation.
                self.active_block = None;
                let input_str = input.to_string();
                let input_preview = format_tool_preview(&name, &input_str);
                vec![DisplayLine::ToolUse {
                    tool: format!("server:{name}"),
                    input_preview,
                }]
            }
            ContentBlock::ServerToolResult { content, .. } => {
                // Server tool result arrives complete — no delta accumulation.
                self.active_block = None;
                extract_tool_result(&content, false).into_iter().collect()
            }
        }
    }

    fn handle_delta(&mut self, delta: Delta) -> Vec<DisplayLine> {
        match delta {
            Delta::TextDelta { text } => {
                self.text_buffer.push_str(&text);
                self.flush_text_lines()
            }
            Delta::InputJsonDelta { partial_json } => {
                self.tool_input_buffer.push_str(&partial_json);
                Vec::new()
            }
            Delta::ThinkingDelta { thinking } => {
                self.thinking_buffer.push_str(&thinking);
                self.flush_thinking_lines()
            }
        }
    }

    fn handle_content_block_stop(&mut self) -> Vec<DisplayLine> {
        let block_type = self.active_block.take();
        match block_type {
            Some(ActiveBlock::Text) => {
                // Flush any remaining text in the buffer.
                let mut lines = Vec::new();
                if !self.text_buffer.is_empty() {
                    lines.push(DisplayLine::Text(self.text_buffer.drain(..).collect()));
                }
                lines
            }
            Some(ActiveBlock::ToolUse) => {
                let tool = self
                    .current_tool
                    .take()
                    .unwrap_or_else(|| "unknown".to_string());
                let input_preview = format_tool_preview(&tool, &self.tool_input_buffer);

                // Capture Edit tool input for diff generation.
                if tool == "Edit" {
                    self.last_edit_input = parse_edit_input(&self.tool_input_buffer);
                } else {
                    self.last_edit_input = None;
                }

                // Detect SendMessage tool calls for inter-agent messaging.
                let agent_msg = if tool == "SendMessage" {
                    parse_agent_message(&self.tool_input_buffer)
                } else {
                    None
                };

                self.tool_input_buffer.clear();

                let mut lines = vec![DisplayLine::ToolUse {
                    tool,
                    input_preview,
                }];

                if let Some((sender, recipient, content)) = agent_msg {
                    lines.push(DisplayLine::AgentMessage {
                        sender,
                        recipient,
                        content,
                    });
                }

                lines
            }
            Some(ActiveBlock::Thinking) => {
                // Flush any remaining thinking text in the buffer.
                let mut lines = Vec::new();
                if !self.thinking_buffer.is_empty() {
                    lines.push(DisplayLine::Thinking(
                        self.thinking_buffer.drain(..).collect(),
                    ));
                }
                lines
            }
            None => Vec::new(),
        }
    }

    /// Flush complete lines (terminated by `\n`) from the text buffer.
    ///
    /// Any trailing content without a newline stays in the buffer for the
    /// next delta or block stop to pick up.
    fn flush_text_lines(&mut self) -> Vec<DisplayLine> {
        let mut lines = Vec::new();

        while let Some(pos) = self.text_buffer.find('\n') {
            let line: String = self.text_buffer.drain(..=pos).collect();
            // Trim the trailing newline for display.
            let text = line.trim_end_matches('\n').to_string();
            lines.push(DisplayLine::Text(text));
        }

        lines
    }

    /// Flush complete lines (terminated by `\n`) from the thinking buffer.
    fn flush_thinking_lines(&mut self) -> Vec<DisplayLine> {
        let mut lines = Vec::new();

        while let Some(pos) = self.thinking_buffer.find('\n') {
            let line: String = self.thinking_buffer.drain(..=pos).collect();
            let text = line.trim_end_matches('\n').to_string();
            lines.push(DisplayLine::Thinking(text));
        }

        lines
    }

    /// Handle a `user` message, extracting tool result content.
    ///
    /// When the most recent tool was an Edit and we captured its input
    /// parameters, a [`DisplayLine::DiffResult`] is emitted alongside the
    /// regular [`DisplayLine::ToolResult`] so the diff view has structured
    /// data to render.
    fn handle_user_message(&mut self, message: UserMessage) -> Vec<DisplayLine> {
        let mut lines = Vec::new();
        for block in message.content {
            match block {
                UserContentBlock::ToolResult {
                    content, is_error, ..
                } => {
                    // Emit a DiffResult if we have captured Edit input.
                    if !is_error {
                        if let Some(edit) = self.last_edit_input.take() {
                            let diff_ops =
                                compute_line_diff(&edit.old_string, &edit.new_string);
                            lines.push(DisplayLine::DiffResult {
                                diff_ops,
                                file_path: edit.file_path,
                            });
                        }
                    } else {
                        // Clear on error — the diff is not meaningful.
                        self.last_edit_input = None;
                    }
                    lines.extend(extract_tool_result(&content, is_error));
                }
                UserContentBlock::Unknown => {}
            }
        }
        lines
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Produce a human-readable summary of a system event.
fn format_system_event(subtype: &str, extra: &serde_json::Value) -> String {
    match subtype {
        "hook_started" => {
            let hook = extra
                .get("hook_name")
                .or_else(|| extra.get("hookName"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            let tool = extra
                .get("tool_name")
                .or_else(|| extra.get("toolName"))
                .and_then(|v| v.as_str())
                .unwrap_or("");
            if tool.is_empty() {
                format!("Hook started: {hook}")
            } else {
                format!("Hook started: {hook} ({tool})")
            }
        }
        "hook_response" => {
            let outcome = extra
                .get("outcome")
                .or_else(|| extra.get("decision"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            format!("Hook response: {outcome}")
        }
        other => format!("System: {other}"),
    }
}

/// Truncate a string to at most [`TOOL_INPUT_PREVIEW_MAX`] characters,
/// appending "…" if truncated.
fn truncate_input(input: &str) -> String {
    // Fast path: input.len() returns byte count, which is always >= char count for UTF-8.
    // If byte length is within the limit, char count is guaranteed to be within it too.
    if input.len() <= TOOL_INPUT_PREVIEW_MAX {
        return input.to_string();
    }
    let boundary = input
        .char_indices()
        .take_while(|&(i, _)| i < TOOL_INPUT_PREVIEW_MAX)
        .last()
        .map_or(0, |(i, c)| i + c.len_utf8());
    format!("{}…", &input[..boundary])
}

/// Format a human-readable preview of a tool call's input parameters.
///
/// For known tools, extracts the most relevant fields and presents them
/// clearly instead of showing raw JSON. Falls back to truncated raw input
/// for unknown tools or when JSON parsing fails.
fn format_tool_preview(tool: &str, raw_input: &str) -> String {
    let value: serde_json::Value = match serde_json::from_str(raw_input) {
        Ok(v) => v,
        Err(_) => return truncate_input(raw_input),
    };

    let preview = match tool {
        "Read" | "Write" | "Edit" => json_str(&value, "file_path")
            .unwrap_or_default()
            .to_string(),

        "NotebookEdit" => json_str(&value, "notebook_path")
            .unwrap_or_default()
            .to_string(),

        "Bash" => {
            if let Some(desc) = json_str(&value, "description") {
                desc.to_string()
            } else {
                json_str(&value, "command").unwrap_or_default().to_string()
            }
        }

        "Grep" => format_grep_preview(&value),

        "Glob" => {
            let pattern = json_str(&value, "pattern").unwrap_or_default();
            match json_str(&value, "path") {
                Some(p) => format!("{pattern} in {p}"),
                None => pattern.to_string(),
            }
        }

        "WebSearch" | "web_search" => {
            let query = json_str(&value, "query").unwrap_or_default();
            format!("\"{query}\"")
        }

        "WebFetch" => json_str(&value, "url").unwrap_or_default().to_string(),

        "Skill" => {
            let skill = json_str(&value, "skill").unwrap_or_default();
            match json_str(&value, "args") {
                Some(args) => format!("{skill} {args}"),
                None => skill.to_string(),
            }
        }

        "Task" => {
            let desc = json_str(&value, "description").unwrap_or_default();
            let agent = json_str(&value, "subagent_type").unwrap_or_default();
            if !agent.is_empty() && !desc.is_empty() {
                format!("[{agent}] {desc}")
            } else if !desc.is_empty() {
                desc.to_string()
            } else {
                agent.to_string()
            }
        }

        "AskUserQuestion" => {
            if let Some(questions) = value.get("questions").and_then(|q| q.as_array()) {
                if let Some(first) = questions.first() {
                    if let Some(q) = first.get("question").and_then(|v| v.as_str()) {
                        return truncate_input(q);
                    }
                }
            }
            truncate_input(raw_input)
        }

        "SendMessage" => {
            let msg_type = json_str(&value, "type").unwrap_or("message");
            let recipient = json_str(&value, "recipient").unwrap_or("?");
            let summary = json_str(&value, "summary").unwrap_or("");
            if !summary.is_empty() {
                format!("{msg_type} to @{recipient}: {summary}")
            } else {
                format!("{msg_type} to @{recipient}")
            }
        }

        "EnterPlanMode" | "ExitPlanMode" => String::new(),

        _ => return truncate_input(raw_input),
    };

    if preview.is_empty() {
        String::new()
    } else {
        truncate_input(&preview)
    }
}

/// Format a preview for `Grep` tool calls.
fn format_grep_preview(value: &serde_json::Value) -> String {
    let pattern = json_str(value, "pattern").unwrap_or_default();
    let mut result = format!("\"{pattern}\"");

    if let Some(glob) = json_str(value, "glob") {
        result.push_str(&format!(" --glob {glob}"));
    }
    if let Some(t) = json_str(value, "type") {
        result.push_str(&format!(" --type {t}"));
    }
    if let Some(p) = json_str(value, "path") {
        result.push_str(&format!(" in {p}"));
    }

    result
}

/// Extract a string value from a JSON object by key.
fn json_str<'a>(value: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(|v| v.as_str())
}

/// Extract text from a tool result content value (string, array of blocks, or null).
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

/// Parse the accumulated JSON input from an Edit tool call into structured
/// parameters for diff generation.
///
/// Returns `Some(EditInput)` when the JSON contains `file_path`, `old_string`,
/// and `new_string` fields. Returns `None` on parse failure or missing fields.
fn parse_edit_input(raw_json: &str) -> Option<EditInput> {
    let value: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    let file_path = json_str(&value, "file_path")?.to_string();
    let old_string = json_str(&value, "old_string")?.to_string();
    let new_string = json_str(&value, "new_string")?.to_string();
    Some(EditInput {
        file_path,
        old_string,
        new_string,
    })
}

/// Parse the accumulated JSON input from a SendMessage tool call into
/// inter-agent message fields.
///
/// Returns `Some((sender, recipient, content))` when the JSON contains the
/// expected fields. The `sender` is inferred as "agent" since the sending
/// agent's name is not included in the tool call itself — the caller can
/// override it if the agent identity is known. The `recipient` comes from
/// the tool input's `recipient` field, and the `content` from the `content`
/// or `summary` field.
fn parse_agent_message(raw_json: &str) -> Option<(String, String, String)> {
    let value: serde_json::Value = serde_json::from_str(raw_json).ok()?;
    let msg_type = json_str(&value, "type").unwrap_or("message");
    // Only process direct messages and broadcasts, not shutdown responses etc.
    if msg_type != "message" && msg_type != "broadcast" {
        return None;
    }
    let recipient = json_str(&value, "recipient")
        .unwrap_or(if msg_type == "broadcast" { "all" } else { "?" })
        .to_string();
    // Prefer summary for a concise display, fall back to full content.
    let content = json_str(&value, "summary")
        .or_else(|| json_str(&value, "content"))
        .unwrap_or("")
        .to_string();
    if content.is_empty() {
        return None;
    }
    Some(("agent".to_string(), recipient, content))
}

/// Convert tool result content into a single [`DisplayLine::ToolResult`].
///
/// The full multiline text is stored as-is in a single `DisplayLine`, so one
/// tool invocation consumes only one ring-buffer slot regardless of how many
/// lines the result contains.
fn extract_tool_result(content: &serde_json::Value, is_error: bool) -> Option<DisplayLine> {
    let text = extract_tool_result_text(content);
    if text.is_empty() {
        return None;
    }
    Some(DisplayLine::ToolResult {
        content: text,
        is_error,
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Text streaming ---------------------------------------------------

    #[test]
    fn text_delta_single_line() {
        let mut p = Parser::new();

        // Start a text content block.
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        );
        assert!(lines.is_empty());

        // Stream a text delta with a complete line.
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello world\n"}}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::Text(t) if t == "Hello world"));
    }

    #[test]
    fn text_delta_accumulation_across_deltas() {
        let mut p = Parser::new();

        // Start text block.
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        );

        // First chunk: no newline, stays in buffer.
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello "}}}"#,
        );
        assert!(lines.is_empty());

        // Second chunk: completes the line.
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"world\n"}}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::Text(t) if t == "Hello world"));
    }

    #[test]
    fn text_delta_multiple_newlines_in_one_delta() {
        let mut p = Parser::new();

        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        );

        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"line1\nline2\nline3"}}}"#,
        );
        // Two complete lines emitted ("line1" and "line2"), "line3" buffered.
        assert_eq!(lines.len(), 2);
        assert!(matches!(&lines[0], DisplayLine::Text(t) if t == "line1"));
        assert!(matches!(&lines[1], DisplayLine::Text(t) if t == "line2"));

        // Block stop flushes the remaining "line3".
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::Text(t) if t == "line3"));
    }

    #[test]
    fn text_block_stop_flushes_buffer() {
        let mut p = Parser::new();

        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        );

        // Partial text without newline.
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"partial"}}}"#,
        );

        // Block stop should flush it.
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::Text(t) if t == "partial"));
    }

    // -- Tool use ---------------------------------------------------------

    #[test]
    fn tool_use_basic() {
        let mut p = Parser::new();

        // Start tool_use block.
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_123","name":"Read","input":{}}}}"#,
        );
        assert!(lines.is_empty());

        // Stream input JSON.
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":"}}}"#,
        );
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"\"/src/main.rs\"}"}}}"#,
        );

        // Block stop emits the tool use with a formatted preview.
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
        );
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DisplayLine::ToolUse {
                tool,
                input_preview,
            } => {
                assert_eq!(tool, "Read");
                // Formatted preview shows just the file path, not raw JSON.
                assert_eq!(input_preview, "/src/main.rs");
            }
            other => panic!("expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn tool_use_input_truncation() {
        let mut p = Parser::new();

        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_456","name":"Write","input":{}}}}"#,
        );

        // Stream a very long input.
        let long_json = "x".repeat(200);
        let delta_json = format!(
            r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":1,"delta":{{"type":"input_json_delta","partial_json":"{long_json}"}}}}}}"#
        );
        p.parse_line(&delta_json);

        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
        );
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DisplayLine::ToolUse {
                tool,
                input_preview,
            } => {
                assert_eq!(tool, "Write");
                // Preview should be truncated to ~100 chars + "…".
                assert!(input_preview.len() <= TOOL_INPUT_PREVIEW_MAX + "…".len());
                assert!(input_preview.ends_with('…'));
            }
            other => panic!("expected ToolUse, got {:?}", other),
        }
    }

    // -- System events ----------------------------------------------------

    #[test]
    fn system_hook_started() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"system","subtype":"hook_started","hook_name":"PreToolUse","hook_id":"abc","tool_name":"Write"}"#,
        );
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DisplayLine::System(s) => {
                assert!(s.contains("Hook started"));
                assert!(s.contains("PreToolUse"));
                assert!(s.contains("Write"));
            }
            other => panic!("expected System, got {:?}", other),
        }
    }

    #[test]
    fn system_hook_response() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"system","subtype":"hook_response","hook_id":"abc","outcome":"approve"}"#,
        );
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DisplayLine::System(s) => {
                assert!(s.contains("Hook response"));
                assert!(s.contains("approve"));
            }
            other => panic!("expected System, got {:?}", other),
        }
    }

    #[test]
    fn system_unknown_subtype() {
        let mut p = Parser::new();
        let lines = p.parse_line(r#"{"type":"system","subtype":"something_else","data":123}"#);
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::System(s) if s.contains("something_else")));
    }

    // -- Error handling ---------------------------------------------------

    #[test]
    fn malformed_json_produces_error() {
        let mut p = Parser::new();
        let lines = p.parse_line("not valid json {{{");
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::Error(s) if s == "not valid json {{{"));
    }

    #[test]
    fn empty_line_produces_nothing() {
        let mut p = Parser::new();
        assert!(p.parse_line("").is_empty());
        assert!(p.parse_line("   ").is_empty());
        assert!(p.parse_line("\n").is_empty());
    }

    // -- Message-level events produce nothing -----------------------------

    #[test]
    fn message_start_emits_turn_start() {
        let mut p = Parser::new();

        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant","content":[],"model":"claude-sonnet-4-20250514","stop_reason":null,"stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":1}}}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::TurnStart));
    }

    #[test]
    fn message_stop_produces_nothing() {
        let mut p = Parser::new();

        let lines = p.parse_line(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#);
        assert!(lines.is_empty());
    }

    #[test]
    fn message_delta_produces_nothing() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":42}}}"#,
        );
        assert!(lines.is_empty());
    }

    // -- Integration: full conversation flow ------------------------------

    #[test]
    fn full_text_and_tool_flow() {
        let mut p = Parser::new();

        // message_start emits TurnStart
        let start_lines = p
            .parse_line(r#"{"type":"stream_event","event":{"type":"message_start","message":{}}}"#);
        assert_eq!(start_lines.len(), 1);
        assert!(matches!(&start_lines[0], DisplayLine::TurnStart));

        // text block
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        );
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"I'll read the file.\n"}}}"#,
        );
        let stop = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        );
        // The newline already flushed the line; stop should produce nothing extra.
        assert!(stop.is_empty());

        // tool_use block
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"toolu_abc","name":"Read","input":{}}}}"#,
        );
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"/src/main.rs\"}"}}}"#,
        );
        let tool_lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
        );
        assert_eq!(tool_lines.len(), 1);
        assert!(matches!(&tool_lines[0], DisplayLine::ToolUse { tool, .. } if tool == "Read"));

        // message_delta + message_stop
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":10}}}"#,
        );
        p.parse_line(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#);
    }

    // -- Assistant event --------------------------------------------------

    #[test]
    fn assistant_text_content_skipped() {
        let mut p = Parser::new();
        // With --include-partial-messages, the assistant event is a duplicate
        // of content already streamed via stream_event deltas — it should be
        // skipped to avoid double output.
        let lines = p.parse_line(
            r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"Hello\nWorld"}],"model":"claude-sonnet-4-20250514","stop_reason":"end_turn","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":5}}}"#,
        );
        assert!(lines.is_empty());
    }

    #[test]
    fn assistant_tool_use_content_skipped() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"/src/main.rs"}}],"model":"claude-sonnet-4-20250514","stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":5}}}"#,
        );
        assert!(lines.is_empty());
    }

    #[test]
    fn assistant_mixed_content_skipped() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"Reading file"},{"type":"tool_use","id":"toolu_1","name":"Read","input":{"file_path":"/x"}}],"model":"claude-sonnet-4-20250514","stop_reason":"tool_use","stop_sequence":null,"usage":{"input_tokens":10,"output_tokens":5}}}"#,
        );
        assert!(lines.is_empty());
    }

    // -- Result event -----------------------------------------------------

    #[test]
    fn result_event_emits_summary() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"result","subtype":"success","result":"Done.","total_cost_usd":0.16,"session_id":"abc-123"}"#,
        );
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DisplayLine::Result(s) => {
                assert!(s.contains("◈ Status: success"));
                assert!(s.contains("◇ Cost: $0.1600"));
                assert!(s.contains("⊙ Session: abc-123"));
            }
            other => panic!("expected Result, got {:?}", other),
        }
    }

    // -- Unknown event types ----------------------------------------------

    #[test]
    fn unknown_type_silently_ignored() {
        let mut p = Parser::new();
        let lines = p.parse_line(r#"{"type":"some_future_event","data":"whatever"}"#);
        assert!(lines.is_empty());
    }

    // -- User tool result events ------------------------------------------

    #[test]
    fn user_tool_result_text_content() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"file contents here\nsecond line"}]}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], DisplayLine::ToolResult { content, is_error } if content == "file contents here\nsecond line" && !is_error)
        );
    }

    #[test]
    fn user_tool_result_array_content() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":[{"type":"text","text":"block text"}]}]}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], DisplayLine::ToolResult { content, is_error } if content == "block text" && !is_error)
        );
    }

    #[test]
    fn user_tool_result_error() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1","content":"error message","is_error":true}]}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], DisplayLine::ToolResult { content, is_error } if content == "error message" && *is_error)
        );
    }

    #[test]
    fn user_tool_result_empty_content() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"toolu_1"}]}}"#,
        );
        assert!(lines.is_empty());
    }

    // -- Thinking blocks --------------------------------------------------

    #[test]
    fn thinking_delta_single_line() {
        let mut p = Parser::new();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}"#,
        );
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"Let me think about this\n"}}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::Thinking(t) if t == "Let me think about this"));
    }

    #[test]
    fn thinking_block_stop_flushes() {
        let mut p = Parser::new();
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"thinking","thinking":""}}}"#,
        );
        p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"thinking_delta","thinking":"partial thought"}}}"#,
        );
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(matches!(&lines[0], DisplayLine::Thinking(t) if t == "partial thought"));
    }

    // -- Server tool use/result -------------------------------------------

    #[test]
    fn server_tool_use_emits_immediately() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"server_tool_use","id":"srvtoolu_1","name":"web_search","input":{"query":"rust async"}}}}"#,
        );
        assert_eq!(lines.len(), 1);
        match &lines[0] {
            DisplayLine::ToolUse {
                tool,
                input_preview,
            } => {
                assert_eq!(tool, "server:web_search");
                // Formatted preview for web_search shows quoted query.
                assert_eq!(input_preview, "\"rust async\"");
            }
            other => panic!("expected ToolUse, got {:?}", other),
        }
    }

    #[test]
    fn server_tool_result_emits_lines() {
        let mut p = Parser::new();
        let lines = p.parse_line(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"server_tool_result","tool_use_id":"srvtoolu_1","content":[{"type":"text","text":"search result line 1\nline 2"}]}}}"#,
        );
        assert_eq!(lines.len(), 1);
        assert!(
            matches!(&lines[0], DisplayLine::ToolResult { content, .. } if content == "search result line 1\nline 2")
        );
    }

    // -- take_session_id --------------------------------------------------

    #[test]
    fn take_session_id_returns_once_then_none() {
        let mut p = Parser::new();

        // Initially, no session ID is available.
        assert_eq!(p.take_session_id(), None);

        // Parse a result event with session_id "abc-123".
        p.parse_line(
            r#"{"type":"result","subtype":"success","result":"Done.","total_cost_usd":0.16,"session_id":"abc-123"}"#,
        );

        // First call returns the session ID.
        assert_eq!(p.take_session_id(), Some("abc-123".to_string()));

        // Second call returns None (take-once semantics).
        assert_eq!(p.take_session_id(), None);

        // Parse another result event with a different session ID.
        p.parse_line(
            r#"{"type":"result","subtype":"success","result":"Done.","total_cost_usd":0.05,"session_id":"def-456"}"#,
        );

        // First call returns the new session ID.
        assert_eq!(p.take_session_id(), Some("def-456".to_string()));

        // Second call returns None again.
        assert_eq!(p.take_session_id(), None);
    }

    #[test]
    fn take_session_id_ignores_non_result_events() {
        let mut p = Parser::new();

        // An assistant event that happens to contain a session_id field
        // should not be captured.
        p.parse_line(
            r#"{"type":"assistant","message":{"content":[],"session_id":"should-ignore"}}"#,
        );
        assert_eq!(p.take_session_id(), None);

        // A system event similarly should be ignored.
        p.parse_line(r#"{"type":"system","message":"init","session_id":"also-ignore"}"#);
        assert_eq!(p.take_session_id(), None);

        // Confirm a result event still works after the ignored events.
        p.parse_line(
            r#"{"type":"result","subtype":"success","result":"Done.","total_cost_usd":0.01,"session_id":"real-id"}"#,
        );
        assert_eq!(p.take_session_id(), Some("real-id".to_string()));
    }
}
