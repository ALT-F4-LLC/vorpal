//! Agent TUI command module.
//!
//! Provides a ratatui-based terminal UI for spawning and managing multiple
//! Claude Code instances across different workspace directories.

mod export;
mod input;
mod manager;
mod markdown;
mod parser;
pub(crate) mod session;
mod state;
pub(crate) mod theme;
mod tui;
mod ui;

pub use manager::ClaudeOptions;
pub use tui::run;
