# Repository Guidelines

## Project Structure & Module Organization
- `cli/`: Rust CLI binary (`vorpal`); entry at `cli/src/main.rs`. Tests colocated in `cli/src/`.
- `sdk/`: Consumer SDKs — `sdk/rust` crate `vorpal-sdk`, `sdk/go`.
- `config/`: Config-driven artifacts and tasks; binary `vorpal-config`.
- `script/`: Dev/CI helpers (`script/dev.sh`, `script/install.sh`).
- Top-level `Cargo.toml` manages the workspace; `rust-toolchain.toml` pins toolchain.
- Prefer small modules; keep tests near code when practical.

## Build, Test, and Development Commands
- `make` / `make build`: Compile the Rust workspace (`TARGET=release` for optimized builds).
- `make check`: Fast type-check via `cargo check`.
- `make format`: Format with `rustfmt`.
- `make lint`: Lint with Clippy; warnings are denied.
- `make test`: Run Rust tests.
- `make generate`: Regenerate Go stubs from protobufs in `sdk/rust/api`.
- Local services: one-time `bash ./script/install.sh` then `./target/debug/vorpal system keys generate`; per session `make vorpal-start`.
- Dev shell: `./script/dev.sh cargo build` (works well with `direnv` + `.envrc`).

## Coding Style & Naming Conventions
- Rust 2021 edition. Run `make format` and `make lint` before PRs.
- Naming: `snake_case` for functions/vars, `UpperCamelCase` for types, `SCREAMING_SNAKE_CASE` for consts.
- Keep modules focused; avoid unnecessary abstractions.

## Testing Guidelines
- Use `#[test]` and async `#[tokio::test]` (see examples in `cli/src/`).
- Name tests `test_*` with clear intent; keep tests hermetic and fast.
- Run locally with `make test`.

## Commit & Pull Request Guidelines
- Commits: concise, imperative. Prefix with `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, or `test:`; include rationale for non-trivial changes.
- PRs: describe what/why, link issues, include screenshots/logs for CLI or UX changes, and pass `make format lint test`.

## Security & Configuration Tips
- Review `script/install.sh` before running (may need elevated privileges).
- User config: `~/.vorpal/Vorpal.toml`; project config at repo root.
- For reproducible/offline builds, vendor crates with `make vendor` and prefer `TARGET=release`.
