//! Agent TUI application state types.
//!
//! Defines the core state structures shared across the TUI: application state,
//! per-agent state, display lines, and event types.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// How long a transient status message remains visible.
const STATUS_MESSAGE_TTL: Duration = Duration::from_secs(3);

/// Maximum number of output lines retained per agent.
pub const MAX_OUTPUT_LINES: usize = 10_000;

// ---------------------------------------------------------------------------
// AppEvent
// ---------------------------------------------------------------------------

/// Central event type for the TUI event loop.
#[derive(Debug)]
pub enum AppEvent {
    /// Parsed output line from an agent process.
    AgentOutput { agent_id: usize, line: DisplayLine },
    /// An agent process terminated.
    AgentExited {
        agent_id: usize,
        exit_code: Option<i32>,
    },
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

/// Whether the TUI is in normal navigation mode or prompt input mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputMode {
    Normal,
    Input,
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
}

impl AgentState {
    /// Create a new agent state in the [`AgentStatus::Running`] state.
    pub fn new(id: usize, workspace: PathBuf, prompt: String) -> Self {
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
    /// Buffer for the prompt input text.
    pub input_buffer: String,
    /// Cursor position within `input_buffer`.
    pub input_cursor: usize,
    /// Monotonic tick counter, incremented each event-loop iteration.
    pub tick: usize,
    /// Default workspace directory for new agents.
    pub default_workspace: PathBuf,
}

impl App {
    /// Create a new, empty application state.
    pub fn new(default_workspace: PathBuf) -> Self {
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
            input_buffer: String::new(),
            input_cursor: 0,
            tick: 0,
            default_workspace,
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
    pub fn next_agent(&mut self) {
        if !self.agents.is_empty() {
            self.focused = (self.focused + 1) % self.agents.len();
        }
    }

    /// Cycle focus to the previous agent (wraps around).
    pub fn prev_agent(&mut self) {
        if !self.agents.is_empty() {
            self.focused = if self.focused == 0 {
                self.agents.len() - 1
            } else {
                self.focused - 1
            };
        }
    }

    /// Enter prompt input mode, clearing any previous buffer content.
    pub fn enter_input_mode(&mut self) {
        self.input_mode = InputMode::Input;
        self.input_buffer.clear();
        self.input_cursor = 0;
    }

    /// Exit prompt input mode, clearing the buffer.
    pub fn exit_input_mode(&mut self) {
        self.input_mode = InputMode::Normal;
        self.input_buffer.clear();
        self.input_cursor = 0;
    }

    /// Submit the current input buffer.
    ///
    /// If the trimmed buffer is non-empty, returns `Some(trimmed_string)` and
    /// exits input mode. Otherwise returns `None` (buffer stays intact so the
    /// user can keep editing).
    pub fn submit_input(&mut self) -> Option<String> {
        let trimmed = self.input_buffer.trim().to_string();
        if trimmed.is_empty() {
            return None;
        }
        self.exit_input_mode();
        Some(trimmed)
    }

    /// Focus a specific agent by index. No-op if out of range.
    pub fn focus_agent(&mut self, index: usize) {
        if index < self.agents.len() {
            self.focused = index;
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
        // Clamp focused index to remain valid.
        if self.agents.is_empty() {
            self.focused = 0;
        } else if self.focused >= self.agents.len() {
            self.focused = self.agents.len() - 1;
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
        let mut agent = AgentState::new(0, PathBuf::from("/tmp/test"), "test".to_string());
        for i in 0..n {
            agent.push_line(DisplayLine::Text(format!("line {i}")));
        }
        agent
    }

    /// Helper: create an App with `n` agents.
    fn app_with_agents(n: usize) -> App {
        let mut app = App::new(PathBuf::from("/tmp/default"));
        for i in 0..n {
            app.add_agent(AgentState::new(
                i,
                PathBuf::from(format!("/tmp/agent-{i}")),
                format!("prompt {i}"),
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
        let mut agent = AgentState::new(0, PathBuf::from("/tmp/test"), "test".to_string());

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
        let mut agent = AgentState::new(0, PathBuf::from("/tmp/test"), "test".to_string());

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
        let mut app = App::new(PathBuf::from("/tmp/default"));
        app.next_agent();
        assert_eq!(app.focused, 0);
    }

    #[test]
    fn prev_agent_noop_when_empty() {
        // No agents — prev_agent should not panic or change focused.
        let mut app = App::new(PathBuf::from("/tmp/default"));
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
        let app = App::new(PathBuf::from("/tmp/default"));
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
        let mut app = App::new(PathBuf::from("/tmp/default"));
        app.enter_input_mode();

        assert_eq!(app.input_mode, InputMode::Input);
        assert!(app.input_buffer.is_empty());
        assert_eq!(app.input_cursor, 0);
    }

    #[test]
    fn enter_input_mode_clears_previous_buffer() {
        let mut app = App::new(PathBuf::from("/tmp/default"));
        app.input_buffer = "leftover".to_string();
        app.input_cursor = 5;

        app.enter_input_mode();

        assert_eq!(app.input_mode, InputMode::Input);
        assert!(app.input_buffer.is_empty());
        assert_eq!(app.input_cursor, 0);
    }

    #[test]
    fn exit_input_mode_restores_normal() {
        let mut app = App::new(PathBuf::from("/tmp/default"));
        app.enter_input_mode();
        app.input_buffer = "some text".to_string();
        app.input_cursor = 4;

        app.exit_input_mode();

        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.input_buffer.is_empty());
        assert_eq!(app.input_cursor, 0);
    }

    #[test]
    fn submit_input_returns_trimmed_text() {
        let mut app = App::new(PathBuf::from("/tmp/default"));
        app.enter_input_mode();
        app.input_buffer = "  hello world  ".to_string();

        let result = app.submit_input();
        assert_eq!(result, Some("hello world".to_string()));
        assert_eq!(app.input_mode, InputMode::Normal);
        assert!(app.input_buffer.is_empty());
        assert_eq!(app.input_cursor, 0);
    }

    #[test]
    fn submit_input_returns_none_for_empty() {
        let mut app = App::new(PathBuf::from("/tmp/default"));
        app.enter_input_mode();
        app.input_buffer = "".to_string();

        let result = app.submit_input();
        assert_eq!(result, None);
        // Should remain in input mode when submission fails.
        assert_eq!(app.input_mode, InputMode::Input);
    }

    #[test]
    fn submit_input_returns_none_for_whitespace_only() {
        let mut app = App::new(PathBuf::from("/tmp/default"));
        app.enter_input_mode();
        app.input_buffer = "   	  ".to_string();

        let result = app.submit_input();
        assert_eq!(result, None);
        assert_eq!(app.input_mode, InputMode::Input);
    }
}
