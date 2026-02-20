//! Theme system for the agent TUI.
//!
//! Extracts all color constants into a [`Theme`] struct so that every color in
//! the TUI is driven by the active theme rather than hardcoded values.
//! Ships with two built-in themes ([`Theme::dark`] and [`Theme::light`]) and
//! supports runtime cycling via the `t` keybinding.

use ratatui::style::Color;

/// All semantic color roles used by the TUI.
///
/// Each field maps to a specific visual element. Rendering code reads from
/// the active `Theme` instead of using inline `Color::*` constants.
#[derive(Debug, Clone)]
pub struct Theme {
    /// Display name shown in the status bar when cycling themes.
    pub name: &'static str,

    // -- Tool category colors ------------------------------------------------
    pub tool_read: Color,
    pub tool_write: Color,
    pub tool_bash: Color,
    pub tool_web: Color,
    pub tool_default: Color,

    // -- Tab bar -------------------------------------------------------------
    pub tab_no_agents: Color,
    pub tab_border: Color,
    pub tab_badge_success: Color,
    pub tab_badge_error: Color,
    pub tab_badge_idle: Color,
    pub tab_badge_spinner: Color,
    pub tab_highlight: Color,
    pub tab_text: Color,
    pub tab_overflow: Color,
    pub tab_title: Color,

    // -- Terminal too small message -------------------------------------------
    pub terminal_too_small: Color,

    // -- Content area --------------------------------------------------------
    pub content_empty: Color,
    pub block_marker: Color,
    pub text_hr: Color,
    pub tool_input_preview: Color,
    pub thinking_color: Color,
    pub result_marker: Color,
    pub result_text: Color,
    pub system_text: Color,
    pub stderr_text: Color,
    pub error_text: Color,
    pub tool_result_text: Color,
    pub tool_result_error: Color,
    pub tool_result_hidden: Color,

    // -- Markdown rendering ---------------------------------------------------
    pub syntect_theme: &'static str,
    pub code_block_bg: Color,
    pub code_block_border: Color,
    pub code_block_lang_label: Color,
    pub table_border: Color,
    pub table_header_fg: Color,
    pub table_header_bg: Color,
    pub inline_code_bg: Color,
    pub inline_code_fg: Color,
    pub blockquote_border: Color,
    pub blockquote_fg: Color,
    pub heading_fg: Color,
    pub link_fg: Color,
    pub list_marker_fg: Color,

    // -- Status bar ----------------------------------------------------------
    pub status_bar_bg: Color,
    pub status_bar_fg: Color,
    pub status_message: Color,
    pub status_no_agent: Color,
    pub activity_idle: Color,
    pub activity_thinking: Color,
    pub activity_tool: Color,
    pub activity_done: Color,
    pub status_separator: Color,
    pub new_output: Color,
    pub hint_bar_bg: Color,
    pub hint_bar_fg: Color,

    // -- Input overlay -------------------------------------------------------
    pub field_label_active: Color,
    pub field_label_inactive: Color,
    pub input_border: Color,
    pub input_title: Color,
    pub input_bg: Color,
    pub input_fg: Color,
    pub cursor_bg: Color,
    pub cursor_fg: Color,
    pub selector_separator: Color,
    pub selector_active: Color,
    pub selector_inactive: Color,
    pub selector_option_dim: Color,
    pub option_unset: Color,
    pub options_header: Color,

    // -- Help overlay --------------------------------------------------------
    pub help_border: Color,
    pub help_title: Color,
    pub help_heading: Color,
    pub help_key: Color,
    pub help_footer: Color,
    pub help_bg: Color,
    pub help_fg: Color,

    // -- Confirm close dialog ------------------------------------------------
    pub confirm_border: Color,
    pub confirm_title: Color,
    pub confirm_text: Color,
    pub confirm_yes: Color,
    pub confirm_no: Color,
    pub confirm_bg: Color,
    pub confirm_fg: Color,

    // -- Welcome screen ------------------------------------------------------
    pub welcome_title: Color,
    pub welcome_description: Color,
    pub welcome_key: Color,
    pub welcome_key_desc: Color,
    pub welcome_border: Color,

    // -- Toast notifications -------------------------------------------------
    pub toast_bg: Color,
    pub toast_fg: Color,
    pub toast_border: Color,
    pub toast_success: Color,
    pub toast_error: Color,

    // -- Tab unread indicator ------------------------------------------------
    pub tab_unread: Color,

    // -- Search mode ---------------------------------------------------------
    pub search_highlight_bg: Color,
    pub search_highlight_fg: Color,
    pub search_current_bg: Color,
    pub search_current_fg: Color,
    pub search_bar_fg: Color,

    // -- Sidebar panel --------------------------------------------------------
    pub sidebar_bg: Color,
    pub sidebar_fg: Color,
    pub sidebar_border: Color,
    pub sidebar_title: Color,
    pub sidebar_selected_bg: Color,
    pub sidebar_selected_fg: Color,
    pub sidebar_status_running: Color,
    pub sidebar_status_done: Color,
    pub sidebar_status_error: Color,
    pub sidebar_dim: Color,

    // -- Diff view -----------------------------------------------------------
    pub diff_addition_fg: Color,
    pub diff_deletion_fg: Color,
    pub diff_header_fg: Color,
    pub diff_context_fg: Color,

    // -- Enhanced diff view --------------------------------------------------
    pub diff_addition_bg: Color,
    pub diff_deletion_bg: Color,
    pub diff_gutter_fg: Color,

    // -- Command palette -----------------------------------------------------
    pub command_bar_fg: Color,
    pub command_match_fg: Color,
    pub command_selected_bg: Color,
    pub command_selected_fg: Color,
    pub command_desc_fg: Color,
    pub command_error_fg: Color,

    // -- User prompt ---------------------------------------------------------
    pub user_prompt_marker: Color,
    pub user_prompt_fg: Color,

    // -- Inline chat input ----------------------------------------------------
    pub chat_input_bg: Color,
    pub chat_input_fg: Color,
    pub chat_input_border: Color,
    pub chat_input_placeholder: Color,
    pub chat_input_focused_border: Color,

    // -- Turn separators -----------------------------------------------------
    pub turn_separator: Color,
    pub turn_separator_label: Color,
    pub turn_separator_meta: Color,

    // -- Tool block containers -----------------------------------------------
    pub tool_block_border: Color,
    pub tool_block_error_border: Color,

    // -- Thinking blocks -----------------------------------------------------
    pub thinking_collapsed_fg: Color,
}

impl Theme {
    /// The default dark theme — matches the original hardcoded colors.
    pub fn dark() -> Self {
        Self {
            name: "dark",

            tool_read: Color::Green,
            tool_write: Color::Yellow,
            tool_bash: Color::Magenta,
            tool_web: Color::Blue,
            tool_default: Color::Cyan,

            tab_no_agents: Color::DarkGray,
            tab_border: Color::DarkGray,
            tab_badge_success: Color::Green,
            tab_badge_error: Color::Red,
            tab_badge_idle: Color::DarkGray,
            tab_badge_spinner: Color::Cyan,
            tab_highlight: Color::Cyan,
            tab_text: Color::White,
            tab_overflow: Color::DarkGray,
            tab_title: Color::White,

            terminal_too_small: Color::Red,

            content_empty: Color::DarkGray,
            block_marker: Color::Cyan,
            text_hr: Color::DarkGray,
            tool_input_preview: Color::DarkGray,
            thinking_color: Color::Magenta,
            result_marker: Color::Blue,
            result_text: Color::Blue,
            system_text: Color::DarkGray,
            stderr_text: Color::DarkGray,
            error_text: Color::Red,
            tool_result_text: Color::Gray,
            tool_result_error: Color::Red,
            tool_result_hidden: Color::Gray,

            syntect_theme: "base16-ocean.dark",
            code_block_bg: Color::Rgb(30, 30, 30),
            code_block_border: Color::DarkGray,
            code_block_lang_label: Color::DarkGray,
            table_border: Color::DarkGray,
            table_header_fg: Color::White,
            table_header_bg: Color::Rgb(40, 40, 40),
            inline_code_bg: Color::Rgb(50, 50, 50),
            inline_code_fg: Color::Rgb(230, 150, 100),
            blockquote_border: Color::DarkGray,
            blockquote_fg: Color::Gray,
            heading_fg: Color::White,
            link_fg: Color::Cyan,
            list_marker_fg: Color::DarkGray,

            status_bar_bg: Color::DarkGray,
            status_bar_fg: Color::White,
            status_message: Color::Yellow,
            status_no_agent: Color::DarkGray,
            activity_idle: Color::DarkGray,
            activity_thinking: Color::Yellow,
            activity_tool: Color::Cyan,
            activity_done: Color::Green,
            status_separator: Color::DarkGray,
            new_output: Color::Yellow,
            hint_bar_bg: Color::Black,
            hint_bar_fg: Color::DarkGray,

            field_label_active: Color::Cyan,
            field_label_inactive: Color::DarkGray,
            input_border: Color::Cyan,
            input_title: Color::Cyan,
            input_bg: Color::Black,
            input_fg: Color::White,
            cursor_bg: Color::White,
            cursor_fg: Color::Black,
            selector_separator: Color::DarkGray,
            selector_active: Color::Cyan,
            selector_inactive: Color::White,
            selector_option_dim: Color::DarkGray,
            option_unset: Color::DarkGray,
            options_header: Color::DarkGray,

            help_border: Color::Cyan,
            help_title: Color::Cyan,
            help_heading: Color::Cyan,
            help_key: Color::Yellow,
            help_footer: Color::DarkGray,
            help_bg: Color::Black,
            help_fg: Color::White,

            confirm_border: Color::Yellow,
            confirm_title: Color::Yellow,
            confirm_text: Color::Yellow,
            confirm_yes: Color::Green,
            confirm_no: Color::Red,
            confirm_bg: Color::Black,
            confirm_fg: Color::White,

            welcome_title: Color::Cyan,
            welcome_description: Color::Gray,
            welcome_key: Color::Yellow,
            welcome_key_desc: Color::DarkGray,
            welcome_border: Color::DarkGray,

            toast_bg: Color::Black,
            toast_fg: Color::White,
            toast_border: Color::Yellow,
            toast_success: Color::Green,
            toast_error: Color::Red,

            tab_unread: Color::Yellow,

            search_highlight_bg: Color::Yellow,
            search_highlight_fg: Color::Black,
            search_current_bg: Color::Rgb(255, 165, 0), // orange
            search_current_fg: Color::Black,
            search_bar_fg: Color::Cyan,

            sidebar_bg: Color::Black,
            sidebar_fg: Color::White,
            sidebar_border: Color::DarkGray,
            sidebar_title: Color::Cyan,
            sidebar_selected_bg: Color::DarkGray,
            sidebar_selected_fg: Color::White,
            sidebar_status_running: Color::Cyan,
            sidebar_status_done: Color::Green,
            sidebar_status_error: Color::Red,
            sidebar_dim: Color::DarkGray,

            diff_addition_fg: Color::Green,
            diff_deletion_fg: Color::Red,
            diff_header_fg: Color::Cyan,
            diff_context_fg: Color::DarkGray,

            diff_addition_bg: Color::Rgb(0, 40, 0),
            diff_deletion_bg: Color::Rgb(40, 0, 0),
            diff_gutter_fg: Color::DarkGray,

            command_bar_fg: Color::Cyan,
            command_match_fg: Color::Yellow,
            command_selected_bg: Color::DarkGray,
            command_selected_fg: Color::White,
            command_desc_fg: Color::Gray,
            command_error_fg: Color::Red,

            user_prompt_marker: Color::Cyan,
            user_prompt_fg: Color::White,

            chat_input_bg: Color::Reset,
            chat_input_fg: Color::White,
            chat_input_border: Color::DarkGray,
            chat_input_placeholder: Color::DarkGray,
            chat_input_focused_border: Color::Cyan,

            turn_separator: Color::DarkGray,
            turn_separator_label: Color::Gray,
            turn_separator_meta: Color::DarkGray,

            tool_block_border: Color::DarkGray,
            tool_block_error_border: Color::Red,

            thinking_collapsed_fg: Color::DarkGray,
        }
    }

    /// A light theme optimized for light terminal backgrounds.
    pub fn light() -> Self {
        Self {
            name: "light",

            tool_read: Color::Green,
            tool_write: Color::Rgb(180, 130, 0),
            tool_bash: Color::Magenta,
            tool_web: Color::Blue,
            tool_default: Color::Rgb(0, 140, 140),

            tab_no_agents: Color::Gray,
            tab_border: Color::Gray,
            tab_badge_success: Color::Green,
            tab_badge_error: Color::Red,
            tab_badge_idle: Color::Gray,
            tab_badge_spinner: Color::Rgb(0, 140, 140),
            tab_highlight: Color::Rgb(0, 140, 140),
            tab_text: Color::Black,
            tab_overflow: Color::Gray,
            tab_title: Color::Black,

            terminal_too_small: Color::Red,

            content_empty: Color::Gray,
            block_marker: Color::Rgb(0, 140, 140),
            text_hr: Color::Gray,
            tool_input_preview: Color::Gray,
            thinking_color: Color::Magenta,
            result_marker: Color::Blue,
            result_text: Color::Blue,
            system_text: Color::Gray,
            stderr_text: Color::Gray,
            error_text: Color::Red,
            tool_result_text: Color::DarkGray,
            tool_result_error: Color::Red,
            tool_result_hidden: Color::DarkGray,

            syntect_theme: "InspiredGitHub",
            code_block_bg: Color::Rgb(245, 245, 245),
            code_block_border: Color::Gray,
            code_block_lang_label: Color::Gray,
            table_border: Color::Gray,
            table_header_fg: Color::Black,
            table_header_bg: Color::Rgb(235, 235, 235),
            inline_code_bg: Color::Rgb(235, 235, 235),
            inline_code_fg: Color::Rgb(180, 80, 50),
            blockquote_border: Color::Gray,
            blockquote_fg: Color::DarkGray,
            heading_fg: Color::Black,
            link_fg: Color::Blue,
            list_marker_fg: Color::Gray,

            status_bar_bg: Color::Rgb(220, 220, 220),
            status_bar_fg: Color::Black,
            status_message: Color::Rgb(180, 130, 0),
            status_no_agent: Color::Gray,
            activity_idle: Color::Gray,
            activity_thinking: Color::Rgb(180, 130, 0),
            activity_tool: Color::Rgb(0, 140, 140),
            activity_done: Color::Green,
            status_separator: Color::Gray,
            new_output: Color::Rgb(180, 130, 0),
            hint_bar_bg: Color::Rgb(240, 240, 240),
            hint_bar_fg: Color::Gray,

            field_label_active: Color::Rgb(0, 140, 140),
            field_label_inactive: Color::Gray,
            input_border: Color::Rgb(0, 140, 140),
            input_title: Color::Rgb(0, 140, 140),
            input_bg: Color::White,
            input_fg: Color::Black,
            cursor_bg: Color::Black,
            cursor_fg: Color::White,
            selector_separator: Color::Gray,
            selector_active: Color::Rgb(0, 140, 140),
            selector_inactive: Color::Black,
            selector_option_dim: Color::Gray,
            option_unset: Color::Gray,
            options_header: Color::Gray,

            help_border: Color::Rgb(0, 140, 140),
            help_title: Color::Rgb(0, 140, 140),
            help_heading: Color::Rgb(0, 140, 140),
            help_key: Color::Rgb(180, 130, 0),
            help_footer: Color::Gray,
            help_bg: Color::White,
            help_fg: Color::Black,

            confirm_border: Color::Rgb(180, 130, 0),
            confirm_title: Color::Rgb(180, 130, 0),
            confirm_text: Color::Rgb(180, 130, 0),
            confirm_yes: Color::Green,
            confirm_no: Color::Red,
            confirm_bg: Color::White,
            confirm_fg: Color::Black,

            welcome_title: Color::Rgb(0, 140, 140),
            welcome_description: Color::DarkGray,
            welcome_key: Color::Rgb(180, 130, 0),
            welcome_key_desc: Color::Gray,
            welcome_border: Color::Gray,

            toast_bg: Color::White,
            toast_fg: Color::Black,
            toast_border: Color::Rgb(180, 130, 0),
            toast_success: Color::Green,
            toast_error: Color::Red,

            tab_unread: Color::Rgb(180, 130, 0),

            search_highlight_bg: Color::Rgb(180, 130, 0),
            search_highlight_fg: Color::Black,
            search_current_bg: Color::Rgb(255, 100, 0),
            search_current_fg: Color::White,
            search_bar_fg: Color::Rgb(0, 140, 140),

            sidebar_bg: Color::White,
            sidebar_fg: Color::Black,
            sidebar_border: Color::Gray,
            sidebar_title: Color::Rgb(0, 140, 140),
            sidebar_selected_bg: Color::Rgb(220, 220, 220),
            sidebar_selected_fg: Color::Black,
            sidebar_status_running: Color::Rgb(0, 140, 140),
            sidebar_status_done: Color::Green,
            sidebar_status_error: Color::Red,
            sidebar_dim: Color::Gray,

            diff_addition_fg: Color::Green,
            diff_deletion_fg: Color::Red,
            diff_header_fg: Color::Rgb(0, 140, 140),
            diff_context_fg: Color::Gray,

            diff_addition_bg: Color::Rgb(220, 255, 220),
            diff_deletion_bg: Color::Rgb(255, 220, 220),
            diff_gutter_fg: Color::Gray,

            command_bar_fg: Color::Rgb(0, 140, 140),
            command_match_fg: Color::Rgb(180, 130, 0),
            command_selected_bg: Color::Rgb(220, 220, 220),
            command_selected_fg: Color::Black,
            command_desc_fg: Color::DarkGray,
            command_error_fg: Color::Red,

            user_prompt_marker: Color::Rgb(0, 140, 140),
            user_prompt_fg: Color::Black,

            chat_input_bg: Color::Reset,
            chat_input_fg: Color::Black,
            chat_input_border: Color::Gray,
            chat_input_placeholder: Color::Gray,
            chat_input_focused_border: Color::Rgb(0, 140, 140),

            turn_separator: Color::Gray,
            turn_separator_label: Color::DarkGray,
            turn_separator_meta: Color::Gray,

            tool_block_border: Color::Gray,
            tool_block_error_border: Color::Red,

            thinking_collapsed_fg: Color::Gray,
        }
    }

    /// All built-in themes, in cycling order.
    pub fn builtins() -> &'static [fn() -> Theme] {
        &[Theme::dark, Theme::light]
    }

    /// Return a display color based on the tool category.
    pub fn tool_color(&self, tool: &str) -> Color {
        let name = tool.strip_prefix("server:").unwrap_or(tool);
        match name {
            "Read" | "Grep" | "Glob" => self.tool_read,
            "Write" | "Edit" | "NotebookEdit" => self.tool_write,
            "Bash" => self.tool_bash,
            "WebSearch" | "WebFetch" | "web_search" => self.tool_web,
            _ => self.tool_default,
        }
    }
}
