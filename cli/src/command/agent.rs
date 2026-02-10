//! Agent TUI command module.
//!
//! Provides a ratatui-based terminal UI for spawning and managing multiple
//! Claude Code instances across different workspace directories.

mod manager;
mod parser;
mod state;
mod tui;
mod ui;

pub use tui::run;
