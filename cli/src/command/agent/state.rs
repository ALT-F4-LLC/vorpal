//! Agent TUI application state types.
//!
//! Defines the core state structures shared across the TUI: application state,
//! per-agent state, display lines, and event types.

use super::manager::ClaudeOptions;
use super::theme::Theme;
use ratatui::layout::Rect;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use tracing::warn;

/// How long a transient status message remains visible.
const STATUS_MESSAGE_TTL: Duration = Duration::from_secs(3);

/// How long a toast notification remains visible before auto-dismissing.
const TOAST_TTL: Duration = Duration::from_secs(5);

/// Maximum number of output lines retained per agent.
pub const MAX_OUTPUT_LINES: usize = 10_000;

/// Valid Claude Code permission modes for the selector.
pub const PERMISSION_MODES: &[&str] = &["default", "plan", "acceptEdits", "bypassPermissions"];

/// Available Claude model options for the selector.
pub const MODELS: &[&str] = &["claude-opus-4-6", "claude-sonnet-4-5", "claude-haiku-4-5"];

/// Available effort levels for the selector.
pub const EFFORT_LEVELS: &[&str] = &["high", "medium", "low"];

// ---------------------------------------------------------------------------
// Command palette
// ---------------------------------------------------------------------------

/// A command registered in the palette.
#[derive(Debug, Clone, Copy)]
pub struct PaletteCommand {
    /// The command name (what the user types).
    pub name: &'static str,
    /// Short description shown in the dropdown.
    pub description: &'static str,
}

/// All available palette commands, in display order.
pub const COMMANDS: &[PaletteCommand] = &[
    PaletteCommand {
        name: "kill",
        description: "Kill the focused agent",
    },
    PaletteCommand {
        name: "new",
        description: "Create a new agent",
    },
    PaletteCommand {
        name: "respond",
        description: "Respond to an exited agent",
    },
    PaletteCommand {
        name: "search",
        description: "Search agent output",
    },
    PaletteCommand {
        name: "help",
        description: "Show keybinding help",
    },
    PaletteCommand {
        name: "quit",
        description: "Close the focused tab",
    },
];

/// Return the commands whose names fuzzy-match the given query.
///
/// A "fuzzy match" means the query characters appear in order within the
/// command name, not necessarily contiguous. An empty query matches all
/// commands.
pub fn filter_commands(query: &str) -> Vec<&'static PaletteCommand> {
    if query.is_empty() {
        return COMMANDS.iter().collect();
    }
    let query_lower = query.to_lowercase();
    COMMANDS
        .iter()
        .filter(|cmd| fuzzy_matches(cmd.name, &query_lower))
        .collect()
}

/// Returns true if all characters in `query` appear in order within `text`.
fn fuzzy_matches(text: &str, query: &str) -> bool {
    let mut text_chars = text.chars();
    for qc in query.chars() {
        loop {
            match text_chars.next() {
                Some(tc) if tc.to_lowercase().next() == Some(qc) => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

// ---------------------------------------------------------------------------
// InputBuffer
// ---------------------------------------------------------------------------

/// A text buffer with an associated cursor position.
///
/// Encapsulates the two fields that every input form field needs (the text
/// content and the byte-offset cursor) together with UTF-8-safe mutation
/// methods. Adding a new form field now requires only one `InputBuffer`
/// field instead of a `String` + `usize` pair.
#[derive(Debug, Clone, Default)]
pub struct InputBuffer {
    text: String,
    cursor: usize,
}

impl InputBuffer {
    /// Create an empty buffer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Return the text content.
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Return the current cursor byte-offset.
    pub fn cursor_pos(&self) -> usize {
        self.cursor
    }

    /// Replace the buffer contents and place the cursor at the end.
    pub fn set_text(&mut self, s: impl Into<String>) {
        self.text = s.into();
        self.cursor = self.text.len();
    }

    /// Clear the buffer and reset the cursor to 0.
    pub fn clear(&mut self) {
        self.text.clear();
        self.cursor = 0;
    }

    /// Insert a character at the current cursor position and advance.
    pub fn insert_char(&mut self, c: char) {
        self.text.insert(self.cursor, c);
        self.cursor += c.len_utf8();
    }

    /// Delete the character before the cursor (Backspace).
    pub fn delete_char(&mut self) {
        if self.cursor > 0 {
            let prev = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
            self.text.remove(prev);
            self.cursor = prev;
        }
    }

    /// Delete the character at the cursor (Delete key).
    pub fn delete_char_forward(&mut self) {
        if self.cursor < self.text.len() {
            self.text.remove(self.cursor);
        }
    }

    /// Move the cursor one character to the left.
    pub fn move_left(&mut self) {
        if self.cursor > 0 {
            self.cursor = self.text[..self.cursor]
                .char_indices()
                .next_back()
                .map(|(i, _)| i)
                .unwrap_or(0);
        }
    }

    /// Move the cursor one character to the right.
    pub fn move_right(&mut self) {
        if self.cursor < self.text.len() {
            let cur = self.cursor;
            self.cursor = self.text[cur..]
                .char_indices()
                .nth(1)
                .map(|(i, _)| cur + i)
                .unwrap_or(self.text.len());
        }
    }

    /// Move the cursor to the start of the buffer.
    pub fn move_home(&mut self) {
        self.cursor = 0;
    }

    /// Move the cursor to the end of the buffer.
    pub fn move_end(&mut self) {
        self.cursor = self.text.len();
    }
}

// ---------------------------------------------------------------------------
// PromptHistory
// ---------------------------------------------------------------------------

/// Maximum number of history entries kept per workspace.
const MAX_HISTORY_ENTRIES: usize = 500;

/// A single saved prompt history entry, storing the full agent configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEntry {
    /// The prompt text.
    pub prompt: String,
    /// The workspace path used.
    pub workspace: String,
    /// Permission mode option.
    pub permission_mode: Option<String>,
    /// Model option.
    pub model: Option<String>,
    /// Effort option.
    pub effort: Option<String>,
    /// Max budget USD option.
    pub max_budget_usd: Option<f64>,
    /// Allowed tools (comma-separated were parsed to Vec).
    pub allowed_tools: Vec<String>,
    /// Additional directories.
    pub add_dirs: Vec<String>,
}

/// Persistent prompt history, scoped to a workspace directory.
///
/// History is stored as a JSON file at `~/.config/vorpal/history/<hash>.json`
/// where `<hash>` is a hex-encoded hash of the workspace path, ensuring
/// different directories have independent histories.
#[derive(Debug)]
pub struct PromptHistory {
    /// Loaded history entries (oldest first).
    entries: Vec<HistoryEntry>,
    /// Path to the history file on disk.
    file_path: PathBuf,
}

impl PromptHistory {
    /// Load (or create) history for the given workspace directory.
    pub fn load(workspace: &std::path::Path) -> Self {
        let file_path = Self::history_file_path(workspace);
        let entries = if file_path.exists() {
            match std::fs::read_to_string(&file_path) {
                Ok(data) => serde_json::from_str::<Vec<HistoryEntry>>(&data).unwrap_or_default(),
                Err(e) => {
                    warn!(?e, "failed to read history file");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        Self { entries, file_path }
    }

    /// Append an entry and persist to disk.
    pub fn push(&mut self, entry: HistoryEntry) {
        // Deduplicate: remove the most recent entry with the same prompt text.
        if let Some(pos) = self.entries.iter().rposition(|e| e.prompt == entry.prompt) {
            self.entries.remove(pos);
        }
        self.entries.push(entry);
        // Trim to capacity.
        if self.entries.len() > MAX_HISTORY_ENTRIES {
            let excess = self.entries.len() - MAX_HISTORY_ENTRIES;
            self.entries.drain(..excess);
        }
        self.save();
    }

    /// Return all entries (oldest first).
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    /// Persist entries to disk.
    ///
    /// Serialization happens synchronously (fast, <1ms for 500 entries),
    /// then the directory creation and atomic file write are offloaded to a
    /// background thread to avoid blocking the event loop.
    ///
    /// The write is atomic: data is written to a temporary file in the same
    /// directory and then renamed into place, preventing truncated files if
    /// two saves race or the process is interrupted mid-write.
    fn save(&self) {
        let data = match serde_json::to_string_pretty(&self.entries) {
            Ok(d) => d,
            Err(e) => {
                warn!(?e, "failed to serialize history");
                return;
            }
        };
        let file_path = self.file_path.clone();
        std::thread::spawn(move || {
            let Some(parent) = file_path.parent() else {
                warn!("history file has no parent directory");
                return;
            };
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(?e, "failed to create history directory");
                return;
            }
            // Write to a temporary file then atomically rename to avoid
            // partial writes from concurrent saves or interrupted I/O.
            let tmp_path = file_path.with_extension("tmp");
            if let Err(e) = std::fs::write(&tmp_path, data) {
                warn!(?e, "failed to write history temp file");
                return;
            }
            if let Err(e) = std::fs::rename(&tmp_path, &file_path) {
                warn!(?e, "failed to rename history temp file");
                // Clean up the temp file on rename failure.
                let _ = std::fs::remove_file(&tmp_path);
            }
        });
    }

    /// Compute the history file path for a workspace.
    fn history_file_path(workspace: &std::path::Path) -> PathBuf {
        let workspace_str = workspace.display().to_string();
        // Simple hash: use the workspace path bytes to produce a hex string.
        let hash = Self::simple_hash(&workspace_str);
        let config_dir = dirs_config_path();
        config_dir.join("history").join(format!("{hash:016x}.json"))
    }

    /// Simple non-cryptographic hash for workspace path keying.
    fn simple_hash(s: &str) -> u64 {
        // FNV-1a 64-bit hash.
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in s.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }
}

/// Return the Vorpal config directory path (`~/.config/vorpal`).
fn dirs_config_path() -> PathBuf {
    if let Some(home) = std::env::var_os("HOME") {
        PathBuf::from(home).join(".config").join("vorpal")
    } else {
        PathBuf::from(".config").join("vorpal")
    }
}

// ---------------------------------------------------------------------------
// AgentTemplate / TemplateStore
// ---------------------------------------------------------------------------

/// A saved agent configuration template.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentTemplate {
    /// Human-readable template name.
    pub name: String,
    /// Optional prompt to pre-fill.
    #[serde(default)]
    pub prompt: String,
    /// Permission mode option.
    #[serde(default)]
    pub permission_mode: Option<String>,
    /// Model option.
    #[serde(default)]
    pub model: Option<String>,
    /// Effort option.
    #[serde(default)]
    pub effort: Option<String>,
    /// Max budget USD option.
    #[serde(default)]
    pub max_budget_usd: Option<f64>,
    /// Allowed tools (comma-separated were parsed to Vec).
    #[serde(default)]
    pub allowed_tools: Vec<String>,
    /// Additional directories.
    #[serde(default)]
    pub add_dirs: Vec<String>,
    /// Whether this is a built-in (non-deletable) template.
    #[serde(default)]
    pub builtin: bool,
}

/// Return the built-in templates that ship with the TUI.
fn builtin_templates() -> Vec<AgentTemplate> {
    vec![
        AgentTemplate {
            name: "reviewer".to_string(),
            prompt: String::new(),
            permission_mode: Some("plan".to_string()),
            model: None,
            effort: Some("high".to_string()),
            max_budget_usd: None,
            allowed_tools: Vec::new(),
            add_dirs: Vec::new(),
            builtin: true,
        },
        AgentTemplate {
            name: "builder".to_string(),
            prompt: String::new(),
            permission_mode: Some("bypassPermissions".to_string()),
            model: None,
            effort: Some("high".to_string()),
            max_budget_usd: None,
            allowed_tools: Vec::new(),
            add_dirs: Vec::new(),
            builtin: true,
        },
        AgentTemplate {
            name: "researcher".to_string(),
            prompt: String::new(),
            permission_mode: Some("plan".to_string()),
            model: None,
            effort: Some("medium".to_string()),
            max_budget_usd: None,
            allowed_tools: Vec::new(),
            add_dirs: Vec::new(),
            builtin: true,
        },
    ]
}

/// Persistent storage for agent configuration templates.
///
/// Templates are stored as a JSON file at `~/.config/vorpal/templates.json`.
/// Built-in templates are always available and cannot be overwritten by
/// user templates with the same name.
#[derive(Debug)]
pub struct TemplateStore {
    /// User-saved templates (loaded from disk).
    user_templates: Vec<AgentTemplate>,
    /// Path to the templates file on disk.
    file_path: PathBuf,
}

impl TemplateStore {
    /// Load templates from disk.
    pub fn load() -> Self {
        let file_path = dirs_config_path().join("templates.json");
        let user_templates = if file_path.exists() {
            match std::fs::read_to_string(&file_path) {
                Ok(data) => serde_json::from_str::<Vec<AgentTemplate>>(&data).unwrap_or_default(),
                Err(e) => {
                    warn!(?e, "failed to read templates file");
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };
        Self {
            user_templates,
            file_path,
        }
    }

    /// Return all templates as owned values: built-ins first, then user-saved.
    pub fn all_templates_owned(&self) -> Vec<AgentTemplate> {
        let mut result = builtin_templates();
        result.extend(self.user_templates.iter().cloned());
        result
    }

    /// Save a new user template. If a user template with the same name exists,
    /// it is replaced.
    pub fn save_template(&mut self, template: AgentTemplate) -> bool {
        // Reject if the name collides with a built-in template.
        if builtin_templates().iter().any(|b| b.name == template.name) {
            return false;
        }
        if let Some(pos) = self
            .user_templates
            .iter()
            .position(|t| t.name == template.name)
        {
            self.user_templates[pos] = template;
        } else {
            self.user_templates.push(template);
        }
        self.persist();
        true
    }

    /// Delete a user template by name. Returns true if found and removed.
    pub fn delete_template(&mut self, name: &str) -> bool {
        if let Some(pos) = self.user_templates.iter().position(|t| t.name == name) {
            self.user_templates.remove(pos);
            self.persist();
            true
        } else {
            false
        }
    }

    /// Persist user templates to disk.
    fn persist(&self) {
        if let Some(parent) = self.file_path.parent() {
            if let Err(e) = std::fs::create_dir_all(parent) {
                warn!(?e, "failed to create templates directory");
                return;
            }
        }
        match serde_json::to_string_pretty(&self.user_templates) {
            Ok(data) => {
                if let Err(e) = std::fs::write(&self.file_path, data) {
                    warn!(?e, "failed to write templates file");
                }
            }
            Err(e) => {
                warn!(?e, "failed to serialize templates");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// InputOverrides
// ---------------------------------------------------------------------------

/// Overrides for pre-populating the input form when responding to an exited
/// agent (rather than creating a brand-new agent from CLI defaults).
pub struct InputOverrides {
    /// Claude Code options to populate the form fields with.
    pub claude_options: ClaudeOptions,
    /// Workspace path to populate the workspace field with.
    pub workspace: PathBuf,
}

// ---------------------------------------------------------------------------
// AgentEvent
// ---------------------------------------------------------------------------

/// Central event type for the TUI event loop.
#[derive(Debug)]
pub enum AgentEvent {
    /// Parsed output line from an agent process.
    Output { agent_id: usize, line: DisplayLine },
    /// An agent process terminated.
    Exited {
        agent_id: usize,
        exit_code: Option<i32>,
    },
    /// The session ID for an agent was resolved (from Claude Code result output).
    SessionId { agent_id: usize, session_id: String },
}

// ---------------------------------------------------------------------------
// Toast
// ---------------------------------------------------------------------------

/// The kind of toast notification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Agent completed successfully (exit code 0).
    Success,
    /// Agent failed (non-zero or unknown exit code).
    Error,
}

/// A transient toast notification displayed when a background agent completes.
#[derive(Debug, Clone)]
pub struct Toast {
    /// Human-readable message (e.g. "Agent 3 completed").
    pub message: String,
    /// Whether this is a success or error toast.
    pub kind: ToastKind,
    /// When the toast was created (for auto-dismiss).
    pub created_at: Instant,
}

impl Toast {
    /// Returns `true` if this toast has exceeded its display duration.
    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() >= TOAST_TTL
    }
}

// ---------------------------------------------------------------------------
// DisplayLine
// ---------------------------------------------------------------------------

/// A single line of rendered output.
#[derive(Debug, Clone)]
pub enum DisplayLine {
    /// Regular text output from Claude.
    Text(String),
    /// A tool-use event (e.g. "Read file.rs").
    ToolUse { tool: String, input_preview: String },
    /// System/hook messages.
    System(String),
    /// Informational stderr output (e.g. tracing/log lines from Claude Code).
    Stderr(String),
    /// Tool result output (what a tool returned).
    ToolResult { content: String, is_error: bool },
    /// Thinking block content (Claude's internal reasoning).
    Thinking(String),
    /// Session result summary (cost, session ID).
    Result(String),
    /// Error messages (malformed stdout protocol lines).
    Error(String),
    /// Marks the start of a new assistant turn (visual separator).
    TurnStart,
}

// ---------------------------------------------------------------------------
// AgentStatus
// ---------------------------------------------------------------------------

/// Lifecycle status of an agent process.
#[derive(Debug, Clone)]
pub enum AgentStatus {
    Running,
    Exited(Option<i32>),
}

// ---------------------------------------------------------------------------
// AgentActivity
// ---------------------------------------------------------------------------

/// What the agent is currently doing, for status display.
#[derive(Debug, Clone)]
pub enum AgentActivity {
    /// Waiting for the next event.
    Idle,
    /// Claude is generating a response.
    Thinking,
    /// Claude is executing a tool.
    Tool(String),
    /// The agent has finished (process exited).
    Done,
}

// ---------------------------------------------------------------------------
// InputMode
// ---------------------------------------------------------------------------

/// How tool result output is displayed in the TUI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultDisplay {
    /// Show first few result lines per run, truncate long content.
    Compact,
    /// Hide result content entirely, show only byte count.
    Hidden,
    /// Show all result lines with full content.
    Full,
}

impl ResultDisplay {
    /// Cycle to the next display mode: Compact → Hidden → Full → Compact.
    pub fn next(self) -> Self {
        match self {
            Self::Compact => Self::Hidden,
            Self::Hidden => Self::Full,
            Self::Full => Self::Compact,
        }
    }

    /// Human-readable label for status messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Hidden => "hidden",
            Self::Full => "full",
        }
    }
}

/// Which pane has focus in split-pane mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitPane {
    /// The left (primary) pane.
    Left,
    /// The right (secondary) pane.
    Right,
}

/// Whether the TUI is in normal navigation mode, prompt input mode, search mode, command mode,
/// history search mode, template picker, or save-template dialog.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Input,
    Search,
    Command,
    HistorySearch,
    TemplatePicker,
    SaveTemplate,
}

/// Which field is active in the new-agent input overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputField {
    Prompt,
    Workspace,
    PermissionMode,
    Model,
    Effort,
    MaxBudgetUsd,
    AllowedTools,
    AddDir,
}

impl InputField {
    /// Advance to the next field in tab order.
    pub fn next(self) -> Self {
        match self {
            Self::Prompt => Self::Workspace,
            Self::Workspace => Self::PermissionMode,
            Self::PermissionMode => Self::Model,
            Self::Model => Self::Effort,
            Self::Effort => Self::MaxBudgetUsd,
            Self::MaxBudgetUsd => Self::AllowedTools,
            Self::AllowedTools => Self::AddDir,
            Self::AddDir => Self::Prompt,
        }
    }

    /// Go to the previous field in tab order.
    pub fn prev(self) -> Self {
        match self {
            Self::Prompt => Self::AddDir,
            Self::Workspace => Self::Prompt,
            Self::PermissionMode => Self::Workspace,
            Self::Model => Self::PermissionMode,
            Self::Effort => Self::Model,
            Self::MaxBudgetUsd => Self::Effort,
            Self::AllowedTools => Self::MaxBudgetUsd,
            Self::AddDir => Self::AllowedTools,
        }
    }
}

// ---------------------------------------------------------------------------
// AgentState
// ---------------------------------------------------------------------------

/// Per-agent state tracking process output, scroll position, and metadata.
#[derive(Debug)]
pub struct AgentState {
    /// Unique agent identifier.
    pub id: usize,
    /// Working directory for this agent.
    pub workspace: PathBuf,
    /// The prompt this agent was started with.
    #[allow(dead_code)]
    pub prompt: String,
    /// Current process lifecycle status.
    pub status: AgentStatus,
    /// Output buffer (capped at [`MAX_OUTPUT_LINES`]).
    pub output: VecDeque<DisplayLine>,
    /// Scroll offset for viewport (0 = bottom/latest).
    pub scroll_offset: usize,
    /// Indicates new output arrived while the user was scrolled up.
    ///
    /// Set to `true` by [`push_line`] when `scroll_offset > 0`. Cleared when
    /// the user scrolls back to the bottom (`scroll_offset` returns to 0).
    pub has_new_output: bool,
    /// Current activity of the agent (for status display).
    pub activity: AgentActivity,
    /// Number of assistant turns observed.
    pub turn_count: usize,
    /// Number of tool-use events observed.
    pub tool_count: usize,
    /// Monotonic counter incremented each time a line is pushed.
    pub output_generation: u64,
    /// Cached collapsed lines from the last render.
    pub cached_lines: Option<Vec<ratatui::text::Line<'static>>>,
    /// Cached wrapped row count from the last render.
    pub cached_row_count: usize,
    /// The output generation when the cache was last computed.
    pub cache_generation: u64,
    /// The result display mode when the cache was last computed.
    pub cache_result_display: Option<ResultDisplay>,
    /// Session ID returned by Claude Code (populated after the process exits).
    pub session_id: Option<String>,
    /// Claude Code options used for this agent's original spawn.
    pub claude_options: ClaudeOptions,
    /// Timestamp when the agent was started, for elapsed time display.
    pub started_at: Instant,
    /// Per-section display mode overrides for individual tool result blocks.
    ///
    /// Keys are the sequential index of the ToolUse line in the output
    /// (0-based, counting only `DisplayLine::ToolUse` variants). When a
    /// section has an entry here, it overrides the global `ResultDisplay`
    /// mode for that specific tool result run. Cleared when the global
    /// `r` key cycles the display mode.
    pub section_overrides: HashMap<usize, ResultDisplay>,
    /// The result display overrides generation when the cache was last computed.
    ///
    /// Incremented each time `section_overrides` is modified so the render
    /// cache can detect staleness.
    pub section_overrides_generation: u64,
    /// The section overrides generation when the cache was last computed.
    pub cache_section_overrides_generation: u64,
}

impl AgentState {
    /// Create a new agent state in the [`AgentStatus::Running`] state.
    pub fn new(
        id: usize,
        workspace: PathBuf,
        prompt: String,
        claude_options: ClaudeOptions,
    ) -> Self {
        Self {
            id,
            workspace,
            prompt,
            status: AgentStatus::Running,
            output: VecDeque::with_capacity(MAX_OUTPUT_LINES),
            scroll_offset: 0,
            has_new_output: false,
            activity: AgentActivity::Idle,
            turn_count: 0,
            tool_count: 0,
            output_generation: 0,
            cached_lines: None,
            cached_row_count: 0,
            cache_generation: 0,
            cache_result_display: None,
            session_id: None,
            claude_options,
            started_at: Instant::now(),
            section_overrides: HashMap::new(),
            section_overrides_generation: 0,
            cache_section_overrides_generation: 0,
        }
    }

    /// Append a line to the output buffer, trimming from the front when over capacity.
    ///
    /// Both `pop_front` and `push_back` are O(1) on `VecDeque`. The ring
    /// buffer is allowed to wrap internally — [`visible_lines`] uses
    /// `VecDeque::range()` to read the correct window without requiring
    /// contiguous memory.
    ///
    /// **Auto-scroll behaviour**: when `scroll_offset` is 0 the viewport is
    /// already pinned to the bottom and new output is visible immediately (no
    /// action needed). When the user has scrolled up (`scroll_offset > 0`),
    /// the offset is left untouched so the viewport stays in place, and
    /// `has_new_output` is set so the UI can show an indicator.
    pub fn push_line(&mut self, line: DisplayLine) {
        if self.output.len() >= MAX_OUTPUT_LINES {
            self.output.pop_front();
            // Keep scroll_offset valid after the oldest line is evicted.
            if self.scroll_offset > 0 {
                self.scroll_offset = self.scroll_offset.saturating_sub(1);
            }
        }
        self.output.push_back(line);
        self.output_generation = self.output_generation.wrapping_add(1);

        // Flag new output when the user is scrolled away from the bottom.
        if self.scroll_offset > 0 {
            self.has_new_output = true;
        }
    }

    /// Toggle the display mode override for a specific tool section.
    ///
    /// If the section currently has an override, it cycles to the next mode.
    /// If the section has no override, it starts by cycling from the given
    /// `global_mode` to the next mode. This creates a per-section override
    /// that differs from the global default.
    pub fn toggle_section(&mut self, section_index: usize, global_mode: ResultDisplay) {
        let current = self
            .section_overrides
            .get(&section_index)
            .copied()
            .unwrap_or(global_mode);
        let next = current.next();
        if next == global_mode {
            // If cycling back to the global mode, remove the override.
            self.section_overrides.remove(&section_index);
        } else {
            self.section_overrides.insert(section_index, next);
        }
        self.section_overrides_generation = self.section_overrides_generation.wrapping_add(1);
        // Invalidate cache since section display changed.
        self.cached_lines = None;
    }

    /// Clear all per-section display overrides (used when the global mode changes).
    pub fn clear_section_overrides(&mut self) {
        if !self.section_overrides.is_empty() {
            self.section_overrides.clear();
            self.section_overrides_generation = self.section_overrides_generation.wrapping_add(1);
            self.cached_lines = None;
        }
    }

    /// Return the lines visible in the viewport given `height` rows.
    ///
    /// `scroll_offset` of 0 means the viewport is pinned to the bottom (latest
    /// lines). Increasing `scroll_offset` scrolls upward into history.
    ///
    /// Uses [`VecDeque::range`] to read the correct window regardless of the
    /// ring buffer's internal layout — no contiguity requirement.
    #[cfg(test)]
    pub fn visible_lines(&self, height: usize) -> Vec<&DisplayLine> {
        let len = self.output.len();
        if len == 0 || height == 0 {
            return Vec::new();
        }

        let max_offset = len.saturating_sub(height);
        let offset = self.scroll_offset.min(max_offset);
        let end = len.saturating_sub(offset);
        let start = end.saturating_sub(height);

        self.output.range(start..end).collect()
    }
}

// ---------------------------------------------------------------------------
// App
// ---------------------------------------------------------------------------

/// Top-level application state for the agent TUI.
#[derive(Debug)]
pub struct App {
    /// All managed agent states.
    pub agents: Vec<AgentState>,
    /// Maps agent id → index in `agents` Vec for O(1) lookup.
    agent_index: HashMap<usize, usize>,
    /// Index of the currently focused agent.
    pub focused: usize,
    /// Whether the application should exit.
    pub should_quit: bool,
    /// Whether the help overlay is visible.
    pub show_help: bool,
    /// Whether a confirmation dialog for closing the focused agent is visible.
    pub confirm_close: bool,
    /// How tool result output is displayed (compact, hidden, or full).
    pub result_display: ResultDisplay,
    /// Transient status message shown in the status bar, with its creation time.
    /// Auto-expires after [`STATUS_MESSAGE_TTL`].
    status_message: Option<(String, Instant)>,
    /// Tracks a pending `g` key press for the vim-style `gg` chord
    /// (scroll-to-top). Stores the [`Instant`] of the first `g` press so
    /// the input handler can enforce a timeout between the two presses.
    pub pending_g: Option<Instant>,
    /// Current input mode (normal navigation vs. prompt input).
    pub input_mode: InputMode,
    /// Which input field is currently active.
    pub input_field: InputField,
    /// Prompt input buffer.
    pub input: InputBuffer,
    /// Workspace input buffer.
    pub workspace: InputBuffer,
    /// Permission mode input buffer.
    pub permission_mode: InputBuffer,
    /// Model input buffer.
    pub model: InputBuffer,
    /// Effort input buffer.
    pub effort: InputBuffer,
    /// Max budget USD input buffer.
    pub max_budget: InputBuffer,
    /// Allowed tools input buffer (comma-separated).
    pub allowed_tools: InputBuffer,
    /// Add-dir input buffer (comma-separated).
    pub add_dir: InputBuffer,
    /// When `Some(id)`, the input overlay is a "respond" to the agent with the
    /// given `id` rather than a new-agent prompt. The target agent's session ID
    /// will be passed to `AgentManager::spawn()` so Claude resumes the conversation.
    /// Stores the agent's `id` (not Vec index) so the reference remains valid even
    /// if agents are added or removed while the input overlay is open.
    pub respond_target: Option<usize>,
    /// Monotonic tick counter, incremented each event-loop iteration.
    pub tick: usize,
    /// Default workspace directory for new agents.
    pub default_workspace: PathBuf,
    /// Default Claude Code options (from CLI flags), used to pre-populate input fields.
    pub default_claude_options: ClaudeOptions,
    /// Active color theme for the TUI.
    pub theme: Theme,
    /// Index into [`Theme::builtins()`] for cycling.
    pub theme_index: usize,
    /// Queue of active toast notifications (newest last).
    pub toasts: Vec<Toast>,
    /// Set of agent IDs that have unread completion events.
    ///
    /// An ID is added when an unfocused agent exits, and removed when
    /// the user switches to that agent's tab.
    pub unread_agents: std::collections::HashSet<usize>,

    /// Whether the input form is in quick-launch mode (prompt-only, minimal
    /// fields) vs advanced mode (all 8 fields). Defaults to `true` when
    /// opening a new agent form via `n`.
    pub quick_launch: bool,

    // -- Template state --------------------------------------------------------
    /// Persistent template store for saving/loading agent configurations.
    pub templates: TemplateStore,
    /// Cached list of templates for the picker (built-ins + user-saved).
    pub template_list: Vec<AgentTemplate>,
    /// Index of the currently selected template in the picker.
    pub template_selected: usize,
    /// Input buffer for the save-template name dialog.
    pub template_name_input: InputBuffer,
    /// Whether the template picker was opened from within the input form
    /// (via Ctrl+P). When true, cancelling the picker returns to input mode
    /// instead of normal mode.
    pub template_picker_from_input: bool,

    // -- Search state --------------------------------------------------------
    /// Search query input buffer (used in [`InputMode::Search`]).
    pub search_query: InputBuffer,
    /// Cached list of match positions: `(line_index, start_byte, end_byte)` tuples
    /// into the cached rendered lines. Byte offsets refer to the original (not
    /// lowercased) flattened span text, so they can safely be used for slicing.
    /// Recomputed when the query or output changes.
    pub search_matches: Vec<(usize, usize, usize)>,
    /// Index into `search_matches` for the currently focused match.
    pub search_match_index: usize,
    /// The output generation when search matches were last computed, for
    /// invalidation when new output arrives.
    pub search_cache_generation: u64,

    // -- Command palette state ------------------------------------------------
    /// Command input buffer (used in [`InputMode::Command`]).
    pub command_input: InputBuffer,
    /// Index of the currently selected command in the filtered list.
    pub command_selected: usize,
    /// Transient error message from executing an unknown command.
    pub command_error: Option<String>,
    /// Cached filtered command list, recomputed on each keypress in command mode.
    /// Used by both the input handler and the render function so the filter only
    /// runs once per event.
    pub command_filtered: Vec<&'static PaletteCommand>,

    // -- Sidebar state ---------------------------------------------------------
    /// Whether the agent sidebar panel is visible.
    pub sidebar_visible: bool,
    /// Selected agent index within the sidebar (for keyboard navigation).
    /// This is independent of `focused` — the sidebar selection can move
    /// without changing the focused agent until Enter is pressed.
    pub sidebar_selected: usize,

    // -- History state -----------------------------------------------------------
    /// Persistent prompt history for the current workspace.
    pub history: PromptHistory,
    /// Current position in the history when cycling with Up/Down arrows in
    /// input mode. `None` means the user is editing a new prompt (not
    /// browsing history). `Some(i)` is an index into the history entries
    /// (0 = most recent).
    pub history_index: Option<usize>,
    /// Saved prompt text before the user started browsing history, so it can
    /// be restored when they press Down past the most recent entry.
    pub history_stash: String,
    /// Search query for Ctrl+R history search.
    pub history_search_query: InputBuffer,
    /// Filtered history results for Ctrl+R search (newest first).
    pub history_search_results: Vec<usize>,
    /// Selected index within `history_search_results`.
    pub history_search_selected: usize,

    // -- Split-pane state -------------------------------------------------------
    /// Whether split-pane mode is active (two agents side by side).
    pub split_enabled: bool,
    /// Vec index of the agent displayed in the right pane. When `None`, the
    /// right pane shows the next agent after `focused` (wrapping).
    pub split_right_index: Option<usize>,
    /// Which pane currently has focus: `Left` or `Right`.
    pub split_focused_pane: SplitPane,

    // -- Mouse state -----------------------------------------------------------
    /// Whether mouse capture is enabled. When true, crossterm captures mouse
    /// events (clicks, scroll). Defaults to `true`.
    pub mouse_enabled: bool,
    /// Cached tab click regions from the last render. Each entry maps a visible
    /// tab's rendered [`Rect`] to the corresponding agent Vec index. Updated by
    /// [`super::ui::render_tabs`] on every frame so click hit-testing stays
    /// current even after tab additions/removals or terminal resizes.
    pub tab_rects: Vec<(Rect, usize)>,
    /// Cached content area [`Rect`] from the last render. Used by mouse scroll
    /// handling to limit scroll events to the output viewport.
    pub content_rect: Option<Rect>,
    /// Cached sidebar agent entry regions from the last render. Each entry
    /// maps a rendered [`Rect`] to the corresponding agent Vec index so
    /// mouse clicks can focus the correct agent from the sidebar.
    pub sidebar_rects: Vec<(Rect, usize)>,
    /// Cached input field regions from the last render of the input overlay.
    /// Maps each rendered field's [`Rect`] to its [`InputField`] variant so
    /// mouse clicks can focus the correct field.
    pub input_field_rects: Vec<(Rect, InputField)>,
}

impl App {
    /// Create a new, empty application state.
    pub fn new(default_workspace: PathBuf, default_claude_options: ClaudeOptions) -> Self {
        let history = PromptHistory::load(&default_workspace);
        Self {
            agents: Vec::new(),
            agent_index: HashMap::new(),
            focused: 0,
            should_quit: false,
            show_help: false,
            confirm_close: false,
            result_display: ResultDisplay::Hidden,
            status_message: None,
            pending_g: None,
            input_mode: InputMode::Normal,
            input_field: InputField::Prompt,
            input: InputBuffer::new(),
            workspace: InputBuffer::new(),
            permission_mode: InputBuffer::new(),
            model: InputBuffer::new(),
            effort: InputBuffer::new(),
            max_budget: InputBuffer::new(),
            allowed_tools: InputBuffer::new(),
            add_dir: InputBuffer::new(),
            respond_target: None,
            tick: 0,
            default_workspace,
            default_claude_options,
            theme: Theme::dark(),
            theme_index: 0,
            toasts: Vec::new(),
            unread_agents: std::collections::HashSet::new(),
            quick_launch: true,
            templates: TemplateStore::load(),
            template_list: Vec::new(),
            template_selected: 0,
            template_name_input: InputBuffer::new(),
            template_picker_from_input: false,
            search_query: InputBuffer::new(),
            search_matches: Vec::new(),
            search_match_index: 0,
            search_cache_generation: 0,
            command_input: InputBuffer::new(),
            command_selected: 0,
            command_error: None,
            command_filtered: COMMANDS.iter().collect(),
            sidebar_visible: false,
            sidebar_selected: 0,
            history,
            history_index: None,
            history_stash: String::new(),
            history_search_query: InputBuffer::new(),
            history_search_results: Vec::new(),
            history_search_selected: 0,
            split_enabled: false,
            split_right_index: None,
            split_focused_pane: SplitPane::Left,
            mouse_enabled: true,
            tab_rects: Vec::new(),
            content_rect: None,
            sidebar_rects: Vec::new(),
            input_field_rects: Vec::new(),
        }
    }

    /// Add an agent and maintain the id → index mapping.
    pub fn add_agent(&mut self, agent: AgentState) {
        let index = self.agents.len();
        self.agent_index.insert(agent.id, index);
        self.agents.push(agent);
    }

    /// Look up a mutable reference to an agent by its id in O(1).
    pub fn agent_by_id_mut(&mut self, agent_id: usize) -> Option<&mut AgentState> {
        self.agent_index
            .get(&agent_id)
            .copied()
            .and_then(|idx| self.agents.get_mut(idx))
    }

    /// Look up the Vec index for an agent id.
    pub fn agent_vec_index(&self, agent_id: usize) -> Option<usize> {
        self.agent_index.get(&agent_id).copied()
    }

    /// Set a transient status message. It will auto-expire after
    /// [`STATUS_MESSAGE_TTL`].
    pub fn set_status_message(&mut self, msg: impl Into<String>) {
        self.status_message = Some((msg.into(), Instant::now()));
    }

    /// Return the current status message if it hasn't expired.
    pub fn status_message(&self) -> Option<&str> {
        self.status_message.as_ref().and_then(|(msg, created)| {
            if created.elapsed() < STATUS_MESSAGE_TTL {
                Some(msg.as_str())
            } else {
                None
            }
        })
    }

    /// Return a reference to the currently focused agent, if any.
    pub fn focused_agent(&self) -> Option<&AgentState> {
        self.agents.get(self.focused)
    }

    /// Return a mutable reference to the currently focused agent, if any.
    pub fn focused_agent_mut(&mut self) -> Option<&mut AgentState> {
        self.agents.get_mut(self.focused)
    }

    /// Cycle focus to the next agent (wraps around).
    ///
    /// Clears the unread indicator for the newly focused agent.
    pub fn next_agent(&mut self) {
        if !self.agents.is_empty() {
            self.focused = (self.focused + 1) % self.agents.len();
            let agent_id = self.agents[self.focused].id;
            self.unread_agents.remove(&agent_id);
        }
    }

    /// Cycle focus to the previous agent (wraps around).
    ///
    /// Clears the unread indicator for the newly focused agent.
    pub fn prev_agent(&mut self) {
        if !self.agents.is_empty() {
            self.focused = if self.focused == 0 {
                self.agents.len() - 1
            } else {
                self.focused - 1
            };
            let agent_id = self.agents[self.focused].id;
            self.unread_agents.remove(&agent_id);
        }
    }

    /// Enter prompt input mode, clearing any previous buffer content.
    ///
    /// Pre-populates workspace and Claude option buffers from the CLI defaults.
    pub fn enter_input_mode(&mut self) {
        self.enter_input_mode_with(None);
    }

    /// Enter prompt input mode with optional overrides for workspace and Claude
    /// options. When `overrides` is `None`, buffers are populated from the CLI
    /// defaults. When `Some`, the provided values are used instead (e.g., when
    /// responding to an exited agent).
    pub fn enter_input_mode_with(&mut self, overrides: Option<InputOverrides>) {
        self.input_mode = InputMode::Input;
        self.input_field = InputField::Prompt;
        self.input.clear();
        // Reset history browsing state.
        self.history_index = None;
        self.history_stash.clear();
        // New agents start in quick-launch mode; respond uses advanced mode
        // since the user likely wants to tweak options.
        self.quick_launch = overrides.is_none();

        let (opts, workspace_str) = match overrides {
            Some(o) => (o.claude_options, o.workspace.display().to_string()),
            None => (
                self.default_claude_options.clone(),
                self.default_workspace.display().to_string(),
            ),
        };

        self.workspace.set_text(workspace_str);
        self.permission_mode
            .set_text(opts.permission_mode.unwrap_or_default());
        self.model.set_text(opts.model.unwrap_or_default());
        self.effort.set_text(opts.effort.unwrap_or_default());
        self.max_budget.set_text(
            opts.max_budget_usd
                .map(|v| v.to_string())
                .unwrap_or_default(),
        );
        self.allowed_tools.set_text(opts.allowed_tools.join(", "));
        self.add_dir.set_text(opts.add_dirs.join(", "));
    }

    /// Exit prompt input mode, clearing all buffers.
    pub fn exit_input_mode(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_field = InputField::Prompt;
        self.input.clear();
        self.workspace.clear();
        self.permission_mode.clear();
        self.model.clear();
        self.effort.clear();
        self.max_budget.clear();
        self.allowed_tools.clear();
        self.add_dir.clear();
        self.respond_target = None;
        self.quick_launch = true;
        self.history_index = None;
        self.history_stash.clear();
    }

    // -- Template methods ------------------------------------------------------

    /// Enter template picker mode. Refreshes the template list and resets
    /// the selection to the first item (the "Blank" option is item 0).
    ///
    /// When `from_input` is true, the picker was opened from within the input
    /// form (via Ctrl+P), so cancelling returns to input mode. When false
    /// (opened from normal mode), cancelling returns to normal mode.
    pub fn enter_template_picker(&mut self, from_input: bool) {
        self.template_list = self.templates.all_templates_owned();
        self.template_selected = 0;
        self.template_picker_from_input = from_input;
        self.input_mode = InputMode::TemplatePicker;
    }

    /// Exit template picker mode.
    ///
    /// Returns to input mode if the picker was opened from the input form,
    /// otherwise returns to normal mode.
    pub fn exit_template_picker(&mut self) {
        if self.template_picker_from_input {
            self.input_mode = InputMode::Input;
        } else {
            self.input_mode = InputMode::Normal;
        }
        self.template_list.clear();
        self.template_selected = 0;
        self.template_picker_from_input = false;
    }

    /// Select the current template and enter input mode with its values
    /// pre-filled. Index 0 is "Blank" (opens the default form). Indices
    /// 1..N correspond to `template_list[i - 1]`.
    ///
    /// When the picker was opened from within the input form, "Blank"
    /// returns to the existing form without resetting fields.
    pub fn select_template(&mut self) {
        let from_input = self.template_picker_from_input;
        self.template_picker_from_input = false;

        if self.template_selected == 0 {
            // "Blank" option.
            self.template_list.clear();
            if from_input {
                // Return to the existing input form without resetting.
                self.input_mode = InputMode::Input;
            } else {
                self.enter_input_mode();
            }
            return;
        }
        let idx = self.template_selected - 1;
        if let Some(template) = self.template_list.get(idx).cloned() {
            self.template_list.clear();
            self.apply_template(&template);
        } else if from_input {
            self.input_mode = InputMode::Input;
        } else {
            self.enter_input_mode();
        }
    }

    /// Apply a template's values to the input form and enter input mode.
    fn apply_template(&mut self, template: &AgentTemplate) {
        self.input_mode = InputMode::Input;
        self.input_field = InputField::Prompt;
        self.history_index = None;
        self.history_stash.clear();
        // Template always opens in advanced mode so the user can see all
        // pre-filled fields.
        self.quick_launch = false;

        self.input.set_text(&template.prompt);
        self.workspace
            .set_text(self.default_workspace.display().to_string());
        self.permission_mode
            .set_text(template.permission_mode.as_deref().unwrap_or_default());
        self.model
            .set_text(template.model.as_deref().unwrap_or_default());
        self.effort
            .set_text(template.effort.as_deref().unwrap_or_default());
        self.max_budget.set_text(
            template
                .max_budget_usd
                .map(|v| v.to_string())
                .unwrap_or_default(),
        );
        self.allowed_tools
            .set_text(template.allowed_tools.join(", "));
        self.add_dir.set_text(template.add_dirs.join(", "));
    }

    /// Enter save-template mode from the input form. The user provides a
    /// name for the template to be saved.
    pub fn enter_save_template(&mut self) {
        self.input_mode = InputMode::SaveTemplate;
        self.template_name_input.clear();
    }

    /// Exit save-template mode, returning to input mode without saving.
    pub fn exit_save_template(&mut self) {
        self.input_mode = InputMode::Input;
        self.template_name_input.clear();
    }

    /// Save the current input form values as a named template.
    pub fn save_current_as_template(&mut self) {
        let name = self.template_name_input.text().trim().to_string();
        if name.is_empty() {
            return;
        }

        let non_empty = |s: &str| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };

        let template = AgentTemplate {
            name: name.clone(),
            prompt: self.input.text().to_string(),
            permission_mode: non_empty(self.permission_mode.text()),
            model: non_empty(self.model.text()),
            effort: non_empty(self.effort.text()),
            max_budget_usd: self
                .max_budget
                .text()
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| *v > 0.0),
            allowed_tools: self
                .allowed_tools
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            add_dirs: self
                .add_dir
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            builtin: false,
        };

        if !self.templates.save_template(template) {
            self.set_status_message(format!("Cannot overwrite built-in template '{name}'"));
            return;
        }
        self.template_name_input.clear();
        self.input_mode = InputMode::Input;
        self.set_status_message(format!("Template '{name}' saved"));
    }

    /// Submit the current input buffers.
    ///
    /// If the trimmed prompt is non-empty, returns
    /// `Some((prompt, workspace, claude_options))` and exits input mode. If the
    /// workspace buffer is empty, the default workspace is used. Claude option
    /// fields are parsed from their buffers (empty = default/None). Returns
    /// `None` when the prompt is empty (buffers stay intact so the user can
    /// keep editing).
    pub fn submit_input(&mut self) -> Option<(String, PathBuf, ClaudeOptions)> {
        let prompt = self.input.text().trim().to_string();
        if prompt.is_empty() {
            return None;
        }
        let workspace_str = self.workspace.text().trim();
        let workspace = if workspace_str.is_empty() {
            self.default_workspace.clone()
        } else {
            PathBuf::from(workspace_str)
        };

        let non_empty = |s: &str| {
            let t = s.trim();
            if t.is_empty() {
                None
            } else {
                Some(t.to_string())
            }
        };

        let claude_options = ClaudeOptions {
            permission_mode: non_empty(self.permission_mode.text()),
            model: non_empty(self.model.text()),
            effort: non_empty(self.effort.text()),
            max_budget_usd: self
                .max_budget
                .text()
                .trim()
                .parse::<f64>()
                .ok()
                .filter(|v| *v > 0.0),
            allowed_tools: self
                .allowed_tools
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            add_dirs: self
                .add_dir
                .text()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
        };

        self.exit_input_mode();
        Some((prompt, workspace, claude_options))
    }

    /// Advance to the next input field (Tab).
    ///
    /// In quick-launch mode, cycles only between Prompt and Workspace.
    pub fn next_input_field(&mut self) {
        if self.quick_launch {
            self.input_field = match self.input_field {
                InputField::Prompt => InputField::Workspace,
                _ => InputField::Prompt,
            };
        } else {
            self.input_field = self.input_field.next();
        }
    }

    /// Go to the previous input field (Shift+Tab).
    ///
    /// In quick-launch mode, cycles only between Prompt and Workspace.
    pub fn prev_input_field(&mut self) {
        if self.quick_launch {
            self.input_field = match self.input_field {
                InputField::Workspace => InputField::Prompt,
                _ => InputField::Workspace,
            };
        } else {
            self.input_field = self.input_field.prev();
        }
    }

    /// Cycle to the next built-in theme.
    pub fn cycle_theme(&mut self) {
        let builtins = Theme::builtins();
        self.theme_index = (self.theme_index + 1) % builtins.len();
        self.theme = builtins[self.theme_index]();
        // Invalidate all agent caches so the new theme colors take effect.
        for agent in &mut self.agents {
            agent.cached_lines = None;
        }
    }

    /// Focus a specific agent by index. No-op if out of range.
    ///
    /// Clears the unread indicator for the newly focused agent.
    pub fn focus_agent(&mut self, index: usize) {
        if index < self.agents.len() {
            self.focused = index;
            let agent_id = self.agents[index].id;
            self.unread_agents.remove(&agent_id);
        }
    }

    /// Push a toast notification onto the queue.
    pub fn push_toast(&mut self, message: String, kind: ToastKind) {
        self.toasts.push(Toast {
            message,
            kind,
            created_at: Instant::now(),
        });
    }

    /// Remove all expired toasts from the queue.
    pub fn expire_toasts(&mut self) {
        self.toasts.retain(|t| !t.is_expired());
    }

    /// Dismiss all active toasts immediately (e.g. on keypress).
    pub fn dismiss_toasts(&mut self) {
        self.toasts.clear();
    }

    /// Enter search mode, clearing any previous search state.
    pub fn enter_search_mode(&mut self) {
        self.input_mode = InputMode::Search;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_index = 0;
        self.search_cache_generation = 0;
    }

    /// Exit search mode, clearing the query and match state.
    pub fn exit_search_mode(&mut self) {
        self.input_mode = InputMode::Normal;
        self.search_query.clear();
        self.search_matches.clear();
        self.search_match_index = 0;
        self.search_cache_generation = 0;
    }

    /// Enter command palette mode, clearing any previous command state.
    pub fn enter_command_mode(&mut self) {
        self.input_mode = InputMode::Command;
        self.command_input.clear();
        self.command_selected = 0;
        self.command_error = None;
        self.command_filtered = filter_commands("");
    }

    /// Exit command palette mode, clearing the input and selection.
    pub fn exit_command_mode(&mut self) {
        self.input_mode = InputMode::Normal;
        self.command_input.clear();
        self.command_selected = 0;
        self.command_error = None;
        self.command_filtered = Vec::new();
    }

    /// Recompute the cached filtered command list from the current input.
    pub fn refilter_commands(&mut self) {
        self.command_filtered = filter_commands(self.command_input.text());
    }

    // -- History methods -------------------------------------------------------

    /// Save a prompt and its configuration to persistent history.
    pub fn save_to_history(
        &mut self,
        prompt: &str,
        workspace: &std::path::Path,
        claude_options: &ClaudeOptions,
    ) {
        let entry = HistoryEntry {
            prompt: prompt.to_string(),
            workspace: workspace.display().to_string(),
            permission_mode: claude_options.permission_mode.clone(),
            model: claude_options.model.clone(),
            effort: claude_options.effort.clone(),
            max_budget_usd: claude_options.max_budget_usd,
            allowed_tools: claude_options.allowed_tools.clone(),
            add_dirs: claude_options.add_dirs.clone(),
        };
        self.history.push(entry);
    }

    /// Navigate to the previous (older) history entry from the prompt field.
    ///
    /// On the first Up press, stashes the current prompt text and loads the
    /// most recent history entry. Subsequent Up presses move further back.
    pub fn history_prev(&mut self) {
        let entries = self.history.entries();
        if entries.is_empty() {
            return;
        }
        match self.history_index {
            None => {
                // Stash current input and go to most recent entry.
                self.history_stash = self.input.text().to_string();
                let idx = 0; // 0 = most recent (we index from the end)
                self.history_index = Some(idx);
                self.apply_history_entry(entries.len() - 1 - idx);
            }
            Some(idx) => {
                let new_idx = idx + 1;
                if new_idx < entries.len() {
                    self.history_index = Some(new_idx);
                    self.apply_history_entry(entries.len() - 1 - new_idx);
                }
                // If already at the oldest entry, do nothing.
            }
        }
    }

    /// Navigate to the next (newer) history entry from the prompt field.
    ///
    /// When the user presses Down past the most recent entry, the stashed
    /// original prompt text is restored.
    pub fn history_next(&mut self) {
        let entries = self.history.entries();
        match self.history_index {
            None => {
                // Not browsing history — do nothing.
            }
            Some(0) => {
                // At most recent entry — restore stashed text.
                self.history_index = None;
                self.input.set_text(&self.history_stash);
            }
            Some(idx) => {
                let new_idx = idx - 1;
                self.history_index = Some(new_idx);
                self.apply_history_entry(entries.len() - 1 - new_idx);
            }
        }
    }

    /// Apply a history entry at the given absolute index (into entries vec)
    /// to the input form fields.
    fn apply_history_entry(&mut self, entry_idx: usize) {
        if let Some(entry) = self.history.entries().get(entry_idx) {
            self.input.set_text(&entry.prompt);
            self.workspace.set_text(&entry.workspace);
            self.permission_mode
                .set_text(entry.permission_mode.as_deref().unwrap_or(""));
            self.model.set_text(entry.model.as_deref().unwrap_or(""));
            self.effort.set_text(entry.effort.as_deref().unwrap_or(""));
            self.max_budget.set_text(
                entry
                    .max_budget_usd
                    .map(|v| v.to_string())
                    .unwrap_or_default(),
            );
            self.allowed_tools.set_text(entry.allowed_tools.join(", "));
            self.add_dir.set_text(entry.add_dirs.join(", "));
        }
    }

    /// Enter history search mode (Ctrl+R from input mode).
    pub fn enter_history_search(&mut self) {
        self.input_mode = InputMode::HistorySearch;
        self.history_search_query.clear();
        self.history_search_results = (0..self.history.entries().len()).rev().collect();
        self.history_search_selected = 0;
    }

    /// Exit history search mode, returning to input mode.
    pub fn exit_history_search(&mut self) {
        self.input_mode = InputMode::Input;
        self.history_search_query.clear();
        self.history_search_results.clear();
        self.history_search_selected = 0;
    }

    /// Recompute the history search results from the current query.
    pub fn refilter_history_search(&mut self) {
        let query = self.history_search_query.text().to_string();
        let entries = self.history.entries();
        if query.is_empty() {
            self.history_search_results = (0..entries.len()).rev().collect();
        } else {
            let query_lower = query.to_lowercase();
            self.history_search_results = entries
                .iter()
                .enumerate()
                .rev()
                .filter(|(_, e)| fuzzy_matches(&e.prompt.to_lowercase(), &query_lower))
                .map(|(i, _)| i)
                .collect();
        }
        self.history_search_selected = 0;
    }

    /// Select a history entry from the search results and populate the form.
    pub fn select_history_search_entry(&mut self) {
        if let Some(&entry_idx) = self
            .history_search_results
            .get(self.history_search_selected)
        {
            self.apply_history_entry(entry_idx);
            self.history_index = None; // Reset browsing state.
        }
        self.exit_history_search();
    }

    /// Recompute search matches against the focused agent's cached lines.
    ///
    /// Stores `(line_index, start_byte, end_byte)` tuples for every substring
    /// match. Byte offsets refer to the original (not lowercased) flattened
    /// span text so they are safe to use for slicing in highlight rendering.
    /// Called when the query changes or the output generation advances.
    pub fn recompute_search_matches(&mut self) {
        self.search_matches.clear();
        self.search_match_index = 0;

        let query = self.search_query.text().to_string();
        if query.is_empty() {
            return;
        }

        let query_lower = query.to_lowercase();

        if let Some(agent) = self.agents.get(self.focused) {
            if let Some(cached) = &agent.cached_lines {
                self.search_cache_generation = agent.output_generation;
                for (line_idx, line) in cached.iter().enumerate() {
                    let text: String = line.spans.iter().map(|s| s.content.as_ref()).collect();

                    // Build a byte-offset mapping from the lowercased text back
                    // to the original text so highlights land on correct char
                    // boundaries even when lowercasing changes byte lengths.
                    //
                    // Assumption: `char::to_lowercase()` never produces fewer
                    // bytes than the original char. This holds for all Unicode
                    // codepoints — lowercasing can expand (e.g. İ → i̇) but
                    // never contracts, so the sentinel at the end is always
                    // correct.
                    let text_lower = text.to_lowercase();
                    let mut lower_to_orig: Vec<usize> = Vec::with_capacity(text_lower.len() + 1);
                    for (orig_byte, ch) in text.char_indices() {
                        for lower_ch in ch.to_lowercase() {
                            for _ in 0..lower_ch.len_utf8() {
                                lower_to_orig.push(orig_byte);
                            }
                        }
                    }
                    lower_to_orig.push(text.len()); // sentinel for end-of-string

                    let mut start = 0;
                    while let Some(pos) = text_lower[start..].find(&query_lower) {
                        let lower_start = start + pos;
                        let lower_end = lower_start + query_lower.len();
                        let orig_start = lower_to_orig[lower_start];
                        let orig_end = if lower_end < lower_to_orig.len() {
                            lower_to_orig[lower_end]
                        } else {
                            text.len()
                        };
                        self.search_matches.push((line_idx, orig_start, orig_end));
                        start = lower_end;
                    }
                }
            }
        }
    }

    /// Jump to the next search match, wrapping around at the end.
    pub fn search_next(&mut self, viewport_height: usize) {
        if !self.search_matches.is_empty() {
            self.search_match_index = (self.search_match_index + 1) % self.search_matches.len();
            self.scroll_to_current_match(viewport_height);
        }
    }

    /// Jump to the previous search match, wrapping around at the beginning.
    pub fn search_prev(&mut self, viewport_height: usize) {
        if !self.search_matches.is_empty() {
            self.search_match_index = if self.search_match_index == 0 {
                self.search_matches.len() - 1
            } else {
                self.search_match_index - 1
            };
            self.scroll_to_current_match(viewport_height);
        }
    }

    /// Scroll the viewport so the current search match is visible.
    ///
    /// `viewport_height` is the number of content rows visible in the output
    /// area (terminal rows minus chrome). The caller should provide this from
    /// [`super::input::viewport_height()`] to avoid a redundant syscall here.
    pub fn scroll_to_current_match(&mut self, viewport_height: usize) {
        if self.search_matches.is_empty() {
            return;
        }
        let (match_line, _, _) = self.search_matches[self.search_match_index];
        if let Some(agent) = self.agents.get_mut(self.focused) {
            let total_rows = agent.cached_row_count;
            let height = viewport_height.max(1);
            let max_offset = total_rows.saturating_sub(height);
            // Convert match_line (from-top) to scroll_offset (from-bottom).
            // scroll_offset 0 = bottom; max_offset = top.
            // We want the match roughly centered in the viewport.
            let from_top = match_line.saturating_sub(height / 2);
            let desired_offset = max_offset.saturating_sub(from_top);
            agent.scroll_offset = desired_offset.min(max_offset);
            if agent.scroll_offset == 0 {
                agent.has_new_output = false;
            }
        }
    }

    /// Return the Vec index of the agent displayed in the right split pane.
    ///
    /// Uses `split_right_index` if set; otherwise defaults to the agent after
    /// `focused` (wrapping). Returns `None` if fewer than 2 agents exist.
    pub fn split_right_agent_index(&self) -> Option<usize> {
        if self.agents.len() < 2 {
            return None;
        }
        if let Some(idx) = self.split_right_index {
            if idx < self.agents.len() && idx != self.focused {
                return Some(idx);
            }
        }
        Some((self.focused + 1) % self.agents.len())
    }

    /// Return a reference to the agent currently active in the focused split pane.
    ///
    /// In split mode, returns the right-pane agent when `split_focused_pane` is
    /// `Right`; otherwise returns the left (primary focused) agent. Outside split
    /// mode, always returns the primary focused agent.
    pub fn active_agent_index(&self) -> Option<usize> {
        if self.split_enabled && self.split_focused_pane == SplitPane::Right {
            self.split_right_agent_index().or(Some(self.focused))
        } else if !self.agents.is_empty() {
            Some(self.focused)
        } else {
            None
        }
    }

    /// Return a mutable reference to the agent in the currently active split pane.
    pub fn active_agent_mut(&mut self) -> Option<&mut AgentState> {
        let idx = self.active_agent_index()?;
        self.agents.get_mut(idx)
    }

    /// Rebuild the `agent_id → Vec index` map.
    ///
    /// Must be called whenever an agent's `id` is changed in place (e.g. when
    /// resuming a session replaces the id with the new process's id).
    pub fn rebuild_agent_index(&mut self) {
        self.agent_index.clear();
        for (i, agent) in self.agents.iter().enumerate() {
            self.agent_index.insert(agent.id, i);
        }
    }

    /// Remove an agent by Vec index, rebuilding the id→index map.
    ///
    /// Returns the removed [`AgentState`] or `None` if the index is out of
    /// range. After removal the focused index is clamped so it stays valid.
    pub fn remove_agent(&mut self, index: usize) -> Option<AgentState> {
        if index >= self.agents.len() {
            return None;
        }
        let removed = self.agents.remove(index);
        // Rebuild the index map since all indices after `index` shifted down.
        self.agent_index.clear();
        for (i, agent) in self.agents.iter().enumerate() {
            self.agent_index.insert(agent.id, i);
        }
        // Remove the agent's ID from unread tracking.
        self.unread_agents.remove(&removed.id);
        // Clamp focused and sidebar_selected indices to remain valid.
        if self.agents.is_empty() {
            self.focused = 0;
            self.sidebar_selected = 0;
        } else {
            if self.focused >= self.agents.len() {
                self.focused = self.agents.len() - 1;
            }
            if self.sidebar_selected >= self.agents.len() {
                self.sidebar_selected = self.agents.len() - 1;
            }
        }
        // Disable split mode if fewer than 2 agents remain.
        if self.agents.len() < 2 {
            self.split_enabled = false;
            self.split_right_index = None;
            self.split_focused_pane = SplitPane::Left;
        } else if let Some(ri) = self.split_right_index {
            if ri >= self.agents.len() || ri == self.focused {
                self.split_right_index = None;
            }
        }
        Some(removed)
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: create an AgentState with `n` text lines pushed.
    fn agent_with_lines(n: usize) -> AgentState {
        let mut agent = AgentState::new(
            0,
            PathBuf::from("/tmp/test"),
            "test".to_string(),
            ClaudeOptions::default(),
        );
        for i in 0..n {
            agent.push_line(DisplayLine::Text(format!("line {i}")));
        }
        agent
    }

    /// Helper: create an App with `n` agents.
    fn app_with_agents(n: usize) -> App {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        for i in 0..n {
            app.add_agent(AgentState::new(
                i,
                PathBuf::from(format!("/tmp/agent-{i}")),
                format!("prompt {i}"),
                ClaudeOptions::default(),
            ));
        }
        app
    }

    // -- App: remove_agent ------------------------------------------------

    #[test]
    fn remove_agent_removes_and_reindexes() {
        let mut app = app_with_agents(3);
        let removed = app.remove_agent(1);
        assert!(removed.is_some());
        assert_eq!(removed.unwrap().id, 1);
        assert_eq!(app.agents.len(), 2);
        assert_eq!(app.agents[0].id, 0);
        assert_eq!(app.agents[1].id, 2);
        // Index map should be rebuilt.
        assert_eq!(app.agent_index[&0], 0);
        assert_eq!(app.agent_index[&2], 1);
    }

    #[test]
    fn remove_agent_clamps_focused() {
        let mut app = app_with_agents(3);
        app.focused = 2;
        app.remove_agent(2);
        assert_eq!(app.focused, 1, "focused should clamp to last valid index");
    }

    #[test]
    fn remove_last_agent_sets_focused_zero() {
        let mut app = app_with_agents(1);
        app.remove_agent(0);
        assert!(app.agents.is_empty());
        assert_eq!(app.focused, 0);
    }

    #[test]
    fn remove_agent_out_of_range_returns_none() {
        let mut app = app_with_agents(2);
        assert!(app.remove_agent(5).is_none());
        assert_eq!(app.agents.len(), 2);
    }

    // -- AgentState: push_line capacity -----------------------------------

    #[test]
    fn push_line_capacity_is_respected() {
        // Push more than MAX_OUTPUT_LINES and verify the buffer is capped.
        let overflow = 100;
        let mut agent = AgentState::new(
            0,
            PathBuf::from("/tmp/test"),
            "test".to_string(),
            ClaudeOptions::default(),
        );

        for i in 0..(MAX_OUTPUT_LINES + overflow) {
            agent.push_line(DisplayLine::Text(format!("line {i}")));
        }

        assert_eq!(
            agent.output.len(),
            MAX_OUTPUT_LINES,
            "output buffer should be capped at MAX_OUTPUT_LINES"
        );

        // The earliest `overflow` lines should have been dropped.
        // The first remaining line should be "line {overflow}".
        match &agent.output[0] {
            DisplayLine::Text(t) => {
                assert_eq!(t, &format!("line {overflow}"));
            }
            other => panic!("expected Text, got {:?}", other),
        }

        // The last line should be the final one pushed.
        let last_idx = MAX_OUTPUT_LINES - 1;
        match &agent.output[last_idx] {
            DisplayLine::Text(t) => {
                assert_eq!(t, &format!("line {}", MAX_OUTPUT_LINES + overflow - 1));
            }
            other => panic!("expected Text, got {:?}", other),
        }
    }

    // -- AgentState: visible_lines ----------------------------------------

    #[test]
    fn visible_lines_returns_last_n_when_at_bottom() {
        // 50 lines, scroll_offset=0 (bottom), viewport height=10.
        // Should return the last 10 lines (lines 40-49).
        let agent = agent_with_lines(50);

        let visible = agent.visible_lines(10);
        assert_eq!(visible.len(), 10);

        // First visible line should be "line 40".
        assert!(matches!(visible[0], DisplayLine::Text(ref t) if t == "line 40"));
        // Last visible line should be "line 49".
        assert!(matches!(visible[9], DisplayLine::Text(ref t) if t == "line 49"));
    }

    #[test]
    fn visible_lines_scrolled_up() {
        // 50 lines, scroll_offset=5, viewport height=10.
        // end = 50 - 5 = 45, start = 45 - 10 = 35.
        // Should return lines 35-44.
        let mut agent = agent_with_lines(50);
        agent.scroll_offset = 5;

        let visible = agent.visible_lines(10);
        assert_eq!(visible.len(), 10);

        assert!(matches!(visible[0], DisplayLine::Text(ref t) if t == "line 35"));
        assert!(matches!(visible[9], DisplayLine::Text(ref t) if t == "line 44"));
    }

    #[test]
    fn visible_lines_empty_output() {
        // No lines at all — should return an empty slice.
        let agent = agent_with_lines(0);
        let visible = agent.visible_lines(10);
        assert!(visible.is_empty());
    }

    #[test]
    fn visible_lines_zero_height() {
        // Viewport height of 0 — should return an empty slice.
        let agent = agent_with_lines(50);
        let visible = agent.visible_lines(0);
        assert!(visible.is_empty());
    }

    #[test]
    fn visible_lines_height_exceeds_output() {
        // Fewer lines than viewport height — should return all lines.
        let agent = agent_with_lines(5);

        let visible = agent.visible_lines(20);
        assert_eq!(visible.len(), 5);
        assert!(matches!(visible[0], DisplayLine::Text(ref t) if t == "line 0"));
        assert!(matches!(visible[4], DisplayLine::Text(ref t) if t == "line 4"));
    }

    #[test]
    fn visible_lines_scroll_offset_exceeds_output() {
        // scroll_offset larger than output — should clamp to top of buffer.
        let mut agent = agent_with_lines(5);
        agent.scroll_offset = 100;

        let visible = agent.visible_lines(10);
        // max_offset = 5.saturating_sub(10) = 0, offset clamped to 0.
        // Shows all 5 lines (fewer than viewport height).
        assert_eq!(visible.len(), 5);
        assert!(matches!(visible[0], DisplayLine::Text(ref t) if t == "line 0"));
        assert!(matches!(visible[4], DisplayLine::Text(ref t) if t == "line 4"));
    }

    #[test]
    fn visible_lines_correct_after_ring_buffer_wraps() {
        // Push more than MAX_OUTPUT_LINES to force pop_front (ring buffer
        // wrap). Then verify visible_lines returns the correct window —
        // including when scrolled up into history.
        let overflow = 200;
        let total = MAX_OUTPUT_LINES + overflow;
        let mut agent = AgentState::new(
            0,
            PathBuf::from("/tmp/test"),
            "test".to_string(),
            ClaudeOptions::default(),
        );

        for i in 0..total {
            agent.push_line(DisplayLine::Text(format!("line {i}")));
        }

        assert_eq!(agent.output.len(), MAX_OUTPUT_LINES);

        // Viewport at the bottom — should see the last 10 lines.
        let visible = agent.visible_lines(10);
        assert_eq!(visible.len(), 10);
        assert!(
            matches!(visible[0], DisplayLine::Text(ref t) if t == &format!("line {}", total - 10))
        );
        assert!(
            matches!(visible[9], DisplayLine::Text(ref t) if t == &format!("line {}", total - 1))
        );

        // Scroll up to the very top of the buffer.
        agent.scroll_offset = MAX_OUTPUT_LINES - 10;
        let visible = agent.visible_lines(10);
        assert_eq!(visible.len(), 10);
        // The first retained line is "line {overflow}".
        assert!(matches!(visible[0], DisplayLine::Text(ref t) if t == &format!("line {overflow}")));
        assert!(
            matches!(visible[9], DisplayLine::Text(ref t) if t == &format!("line {}", overflow + 9))
        );
    }

    // -- App: focus cycling -----------------------------------------------

    #[test]
    fn next_agent_cycles_through_all() {
        // 3 agents, cycling 4 times should wrap: 0 -> 1 -> 2 -> 0 -> 1.
        let mut app = app_with_agents(3);
        assert_eq!(app.focused, 0);

        app.next_agent();
        assert_eq!(app.focused, 1);

        app.next_agent();
        assert_eq!(app.focused, 2);

        app.next_agent();
        assert_eq!(app.focused, 0, "should wrap around to 0");

        app.next_agent();
        assert_eq!(app.focused, 1);
    }

    #[test]
    fn prev_agent_cycles_backward() {
        // 3 agents, prev from 0 should wrap to 2.
        let mut app = app_with_agents(3);
        assert_eq!(app.focused, 0);

        app.prev_agent();
        assert_eq!(app.focused, 2, "should wrap around to last agent");

        app.prev_agent();
        assert_eq!(app.focused, 1);

        app.prev_agent();
        assert_eq!(app.focused, 0);
    }

    #[test]
    fn next_agent_noop_when_empty() {
        // No agents — next_agent should not panic or change focused.
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.next_agent();
        assert_eq!(app.focused, 0);
    }

    #[test]
    fn prev_agent_noop_when_empty() {
        // No agents — prev_agent should not panic or change focused.
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.prev_agent();
        assert_eq!(app.focused, 0);
    }

    // -- App: focus_agent -------------------------------------------------

    #[test]
    fn focus_agent_sets_index() {
        let mut app = app_with_agents(3);
        app.focus_agent(2);
        assert_eq!(app.focused, 2);
    }

    #[test]
    fn focus_agent_out_of_range_is_noop() {
        // Out-of-range index should be silently ignored.
        let mut app = app_with_agents(3);
        app.focus_agent(5);
        assert_eq!(app.focused, 0, "focused should remain unchanged");
    }

    // -- App: focused_agent accessors -------------------------------------

    #[test]
    fn focused_agent_returns_correct_agent() {
        let app = app_with_agents(3);
        let agent = app.focused_agent().expect("should have focused agent");
        assert_eq!(agent.id, 0);
    }

    #[test]
    fn focused_agent_returns_none_when_empty() {
        let app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        assert!(app.focused_agent().is_none());
    }

    #[test]
    fn focused_agent_mut_can_modify() {
        let mut app = app_with_agents(3);
        {
            let agent = app.focused_agent_mut().expect("should have focused agent");
            agent.push_line(DisplayLine::Text("mutated".to_string()));
        }
        assert_eq!(app.agents[0].output.len(), 1);
    }

    // -- App: input mode -----------------------------------------------------

    #[test]
    fn enter_input_mode_sets_state() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();

        assert_eq!(app.input_mode, InputMode::Input);
        assert_eq!(app.input_field, InputField::Prompt);
        assert!(app.input.text().is_empty());
        assert_eq!(app.input.cursor_pos(), 0);
        assert_eq!(app.workspace.text(), "/tmp/default");
        assert_eq!(app.workspace.cursor_pos(), "/tmp/default".len());
    }

    #[test]
    fn enter_input_mode_clears_previous_buffer() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.input.set_text("leftover");
        app.workspace.set_text("/old/path");
        app.input_field = InputField::Workspace;

        app.enter_input_mode();

        assert_eq!(app.input_mode, InputMode::Input);
        assert_eq!(app.input_field, InputField::Prompt);
        assert!(app.input.text().is_empty());
        assert_eq!(app.input.cursor_pos(), 0);
        assert_eq!(app.workspace.text(), "/tmp/default");
        assert_eq!(app.workspace.cursor_pos(), "/tmp/default".len());
    }

    #[test]
    fn exit_input_mode_restores_normal() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();
        app.input.set_text("some text");
        app.workspace.set_text("/some/path");
        app.input_field = InputField::Workspace;

        app.exit_input_mode();

        assert_eq!(app.input_mode, InputMode::Normal);
        assert_eq!(app.input_field, InputField::Prompt);
        assert!(app.input.text().is_empty());
        assert_eq!(app.input.cursor_pos(), 0);
        assert!(app.workspace.text().is_empty());
        assert_eq!(app.workspace.cursor_pos(), 0);
    }

    #[test]
    fn submit_input_returns_trimmed_text() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();
        app.input.set_text("  hello world  ");

        let result = app.submit_input();
        let (prompt, workspace, _opts) = result.expect("should return Some");
        assert_eq!(prompt, "hello world");
        assert_eq!(workspace, PathBuf::from("/tmp/default"));
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.input.text().is_empty());
        assert_eq!(app.input.cursor_pos(), 0);
    }

    #[test]
    fn submit_input_returns_none_for_empty() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();
        // input is already empty after enter_input_mode

        let result = app.submit_input();
        assert_eq!(result, None);
        // Should remain in input mode when submission fails.
        assert_eq!(app.input_mode, InputMode::Input);
    }

    #[test]
    fn submit_input_returns_none_for_whitespace_only() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();
        app.input.set_text("   \t  ");

        let result = app.submit_input();
        assert_eq!(result, None);
        assert_eq!(app.input_mode, InputMode::Input);
    }

    #[test]
    fn submit_input_uses_custom_workspace() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();
        app.input.set_text("my prompt");
        app.workspace.set_text("/custom/path");

        let result = app.submit_input();
        let (prompt, workspace, _opts) = result.expect("should return Some");
        assert_eq!(prompt, "my prompt");
        assert_eq!(workspace, PathBuf::from("/custom/path"));
    }

    #[test]
    fn submit_input_falls_back_to_default_workspace() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();
        app.input.set_text("my prompt");
        app.workspace.set_text("   "); // whitespace-only

        let result = app.submit_input();
        let (prompt, workspace, _opts) = result.expect("should return Some");
        assert_eq!(prompt, "my prompt");
        assert_eq!(workspace, PathBuf::from("/tmp/default"));
    }

    #[test]
    fn input_field_cycles_forward_and_backward_advanced() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();
        app.quick_launch = false; // advanced mode cycles all 8 fields

        assert_eq!(app.input_field, InputField::Prompt);
        app.next_input_field();
        assert_eq!(app.input_field, InputField::Workspace);
        app.next_input_field();
        assert_eq!(app.input_field, InputField::PermissionMode);
        // Cycle backward.
        app.prev_input_field();
        assert_eq!(app.input_field, InputField::Workspace);
        app.prev_input_field();
        assert_eq!(app.input_field, InputField::Prompt);
        // Wrap backward from Prompt → AddDir.
        app.prev_input_field();
        assert_eq!(app.input_field, InputField::AddDir);
        // Wrap forward from AddDir → Prompt.
        app.next_input_field();
        assert_eq!(app.input_field, InputField::Prompt);
    }

    #[test]
    fn input_field_cycles_quick_launch() {
        let mut app = App::new(PathBuf::from("/tmp/default"), ClaudeOptions::default());
        app.enter_input_mode();
        assert!(app.quick_launch, "new agent form defaults to quick launch");

        assert_eq!(app.input_field, InputField::Prompt);
        app.next_input_field();
        assert_eq!(app.input_field, InputField::Workspace);
        // Wrap forward from Workspace → Prompt.
        app.next_input_field();
        assert_eq!(app.input_field, InputField::Prompt);
        // Wrap backward from Prompt → Workspace.
        app.prev_input_field();
        assert_eq!(app.input_field, InputField::Workspace);
        app.prev_input_field();
        assert_eq!(app.input_field, InputField::Prompt);
    }

    // -- fuzzy_matches -------------------------------------------------------

    #[test]
    fn fuzzy_matches_exact() {
        assert!(fuzzy_matches("kill", "kill"));
    }

    #[test]
    fn fuzzy_matches_subsequence() {
        assert!(fuzzy_matches("respond", "rsp"));
    }

    #[test]
    fn fuzzy_matches_no_match() {
        assert!(!fuzzy_matches("kill", "z"));
    }

    #[test]
    fn fuzzy_matches_empty_query() {
        assert!(fuzzy_matches("anything", ""));
    }

    #[test]
    fn fuzzy_matches_case_insensitive() {
        assert!(fuzzy_matches("Help", "help"));
        assert!(fuzzy_matches("QUIT", "quit"));
    }

    #[test]
    fn fuzzy_matches_query_longer_than_text() {
        assert!(!fuzzy_matches("hi", "help"));
    }

    #[test]
    fn fuzzy_matches_single_char() {
        assert!(fuzzy_matches("search", "s"));
        assert!(!fuzzy_matches("kill", "z"));
    }

    #[test]
    fn fuzzy_matches_order_matters() {
        // Characters must appear in order: "lk" cannot match "kill".
        assert!(!fuzzy_matches("kill", "lk"));
    }

    // -- filter_commands -----------------------------------------------------

    #[test]
    fn filter_commands_empty_query_returns_all() {
        let results = filter_commands("");
        assert_eq!(results.len(), COMMANDS.len());
    }

    #[test]
    fn filter_commands_exact_match() {
        let results = filter_commands("kill");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "kill");
    }

    #[test]
    fn filter_commands_no_match() {
        let results = filter_commands("zzz");
        assert!(results.is_empty());
    }

    // -- history navigation --------------------------------------------------

    #[test]
    fn history_prev_next_cycles_entries() {
        let mut app = App::new(
            PathBuf::from("/tmp/test-history-prev-next"),
            ClaudeOptions::default(),
        );
        app.enter_input_mode();

        // Manually insert history entries.
        app.history.push(HistoryEntry {
            prompt: "first".into(),
            workspace: "/tmp/test-history-prev-next".into(),
            permission_mode: None,
            model: None,
            effort: None,
            max_budget_usd: None,
            allowed_tools: Vec::new(),
            add_dirs: Vec::new(),
        });
        app.history.push(HistoryEntry {
            prompt: "second".into(),
            workspace: "/tmp/test-history-prev-next".into(),
            permission_mode: None,
            model: None,
            effort: None,
            max_budget_usd: None,
            allowed_tools: Vec::new(),
            add_dirs: Vec::new(),
        });

        app.input.set_text("current");

        // Up → most recent ("second")
        app.history_prev();
        assert_eq!(app.input.text(), "second");
        assert_eq!(app.history_index, Some(0));

        // Up again → older ("first")
        app.history_prev();
        assert_eq!(app.input.text(), "first");
        assert_eq!(app.history_index, Some(1));

        // Down → back to "second"
        app.history_next();
        assert_eq!(app.input.text(), "second");
        assert_eq!(app.history_index, Some(0));

        // Down again → restore stashed "current"
        app.history_next();
        assert_eq!(app.input.text(), "current");
        assert_eq!(app.history_index, None);
    }

    #[test]
    fn history_prev_on_empty_history_is_noop() {
        let mut app = App::new(
            PathBuf::from("/tmp/test-history-empty"),
            ClaudeOptions::default(),
        );
        app.enter_input_mode();
        app.input.set_text("my prompt");

        app.history_prev();
        assert_eq!(app.input.text(), "my prompt");
        assert_eq!(app.history_index, None);
    }

    #[test]
    fn apply_history_entry_clears_missing_fields() {
        let mut app = App::new(
            PathBuf::from("/tmp/test-history-clears"),
            ClaudeOptions::default(),
        );
        app.enter_input_mode();

        // Pre-fill some fields.
        app.model.set_text("opus");
        app.effort.set_text("high");
        app.max_budget.set_text("5.0");

        // Insert a history entry with no optional fields.
        app.history.push(HistoryEntry {
            prompt: "bare prompt".into(),
            workspace: "/tmp/test-history-clears".into(),
            permission_mode: None,
            model: None,
            effort: None,
            max_budget_usd: None,
            allowed_tools: Vec::new(),
            add_dirs: Vec::new(),
        });

        // Navigate to it.
        app.history_prev();
        assert_eq!(app.input.text(), "bare prompt");
        // Fields should be cleared, not retain previous values.
        assert_eq!(app.model.text(), "");
        assert_eq!(app.effort.text(), "");
        assert_eq!(app.max_budget.text(), "");
    }

    // -- template store ------------------------------------------------------

    #[test]
    fn template_store_rejects_builtin_name() {
        let mut store = TemplateStore::load();
        let result = store.save_template(AgentTemplate {
            name: "reviewer".into(),
            prompt: "custom".into(),
            permission_mode: None,
            model: None,
            effort: None,
            max_budget_usd: None,
            allowed_tools: Vec::new(),
            add_dirs: Vec::new(),
            builtin: false,
        });
        assert!(!result, "should reject saving with built-in name");
    }

    // -- split pane ----------------------------------------------------------

    #[test]
    fn split_right_agent_index_defaults_to_next() {
        let mut app = App::new(PathBuf::from("/tmp/ws"), ClaudeOptions::default());
        app.agents.push(AgentState::new(
            1,
            PathBuf::from("/tmp/ws"),
            "a".into(),
            ClaudeOptions::default(),
        ));
        app.agents.push(AgentState::new(
            2,
            PathBuf::from("/tmp/ws"),
            "b".into(),
            ClaudeOptions::default(),
        ));
        app.agents.push(AgentState::new(
            3,
            PathBuf::from("/tmp/ws"),
            "c".into(),
            ClaudeOptions::default(),
        ));
        app.focused = 0;

        assert_eq!(app.split_right_agent_index(), Some(1));

        app.focused = 2;
        assert_eq!(app.split_right_agent_index(), Some(0)); // wraps
    }

    #[test]
    fn split_right_agent_index_none_with_one_agent() {
        let mut app = App::new(PathBuf::from("/tmp/ws"), ClaudeOptions::default());
        app.agents.push(AgentState::new(
            1,
            PathBuf::from("/tmp/ws"),
            "a".into(),
            ClaudeOptions::default(),
        ));
        assert_eq!(app.split_right_agent_index(), None);
    }

    // -- sidebar sync --------------------------------------------------------

    #[test]
    fn remove_agent_clamps_sidebar_selected() {
        let mut app = App::new(PathBuf::from("/tmp/ws"), ClaudeOptions::default());
        app.agents.push(AgentState::new(
            1,
            PathBuf::from("/tmp/ws"),
            "a".into(),
            ClaudeOptions::default(),
        ));
        app.agents.push(AgentState::new(
            2,
            PathBuf::from("/tmp/ws"),
            "b".into(),
            ClaudeOptions::default(),
        ));
        app.rebuild_agent_index();
        app.sidebar_selected = 1;

        app.remove_agent(1);
        assert!(app.sidebar_selected < app.agents.len());
    }
}
