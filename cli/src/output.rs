//! Single sanctioned path for the CLI's user-facing terminal output.
//!
//! Every module that needs to print information to the user routes through
//! this module instead of calling `println!`/`eprintln!` directly, so the
//! `print_stdout`/`print_stderr` lints have exactly one place to allow.

/// Writes one line to standard output.
#[expect(
    clippy::print_stdout,
    reason = "the CLI's single sanctioned stdout path; every other module routes user-facing output through here"
)]
pub fn line(line: impl std::fmt::Display) {
    println!("{line}");
}
