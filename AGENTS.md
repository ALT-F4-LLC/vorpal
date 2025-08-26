# Repository Guidelines

## Project Structure & Module Organization
- `cli/`: Rust CLI binary `vorpal`; entry `cli/src/main.rs`. Tests live in `cli/src/`.
- `sdk/`: Consumer SDKs — Rust crate at `sdk/rust/vorpal-sdk` and `sdk/go`.
- `config/`: Config‑driven artifacts and tasks; binary `vorpal-config`.
- `script/`: Dev/CI helpers (`script/dev.sh`, `script/install.sh`).
- Workspace is managed by top‑level `Cargo.toml`; toolchain pinned by `rust-toolchain.toml`.

## Build, Test, and Development Commands
- `make` / `make build`: Compile the Rust workspace (`TARGET=release` for optimized builds).
- `make check`: Fast type check via `cargo check`.
- `make format`: Format with `rustfmt`.
- `make lint`: Lint with Clippy (warnings denied).
- `make test`: Run Rust tests.
- `make generate`: Regenerate Go stubs from protobufs in `sdk/rust/api`.
- Local services: one‑time `bash ./script/install.sh` then `./target/debug/vorpal system keys generate`; per session `make vorpal-start`.
- Dev shell: `./script/dev.sh cargo build` (works with `direnv` + `.envrc`).

## Coding Style & Naming Conventions
- Rust 2021; keep modules small and focused.
- Naming: `snake_case` (functions/vars), `UpperCamelCase` (types), `SCREAMING_SNAKE_CASE` (consts).
- Prefer clarity over cleverness. Run `make format lint` before PRs.

## Testing Guidelines
- Use `#[test]` and async `#[tokio::test]`; keep tests hermetic and fast.
- Name tests `test_*` with clear intent; colocate near code when practical (e.g., `cli/src/`).
- Run locally with `make test`.

## Commit & Pull Request Guidelines
- Commits: concise, imperative; prefix with `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, or `test:`. Include rationale for non‑trivial changes.
- PRs: explain what/why, link issues, and include screenshots/logs for CLI or UX changes.
- PRs must pass `make format lint test`.

## Security & Configuration Tips
- Review `script/install.sh` before running (may need elevated privileges).
- User config: `~/.vorpal/Vorpal.toml`; project config lives at repo root.
- For reproducible/offline builds, vendor crates with `make vendor` and prefer `TARGET=release`.
