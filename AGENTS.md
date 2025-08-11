# Repository Guidelines

## Project Structure & Module Organization
- `cli/`: Rust CLI binary (`vorpal`); entry at `cli/src/main.rs`.
- `sdk/`: Consumer SDKs (`sdk/rust` crate `vorpal-sdk`, `sdk/go`).
- `config/`: Config-driven artifacts and tasks; binary `vorpal-config`.
- `script/`: Dev/CI helpers (`script/dev.sh`, `script/install.sh`).
- Workspace managed by top-level `Cargo.toml`; toolchain pinned in `rust-toolchain.toml`.
- Tests are colocated with code where practical (see examples in `cli/src/`).

## Build, Test, and Development Commands
- `make` / `make build`: Compile the Rust workspace (`TARGET=release` for optimized builds).
- `make check`: Fast type-check via `cargo check`.
- `make format`: Format with `rustfmt`.
- `make lint`: Lint with Clippy; warnings are denied.
- `make test`: Run Rust tests.
- `make generate`: Regenerate Go stubs from protobufs in `sdk/rust/api`.
- `make vorpal-start`: Start local services on `localhost:23152`.
- `make vorpal`: Build the repo using Vorpal itself.
- Dev shell: `./script/dev.sh cargo build` (works well with `direnv` + `.envrc`).

Local services are required for registry/worker interactions (e.g., `make vorpal`). One-time: `bash ./script/install.sh` then `./target/debug/vorpal system keys generate`. Per session: `make vorpal-start`. Pure Rust tasks (build/test/lint/format) do not require services.

## Coding Style & Naming Conventions
- Rust 2021 edition. Run `cargo fmt` and Clippy in CI.
- Naming: `snake_case` for functions/vars, `UpperCamelCase` for types, `SCREAMING_SNAKE_CASE` for consts.
- Keep modules small; colocate tests when feasible.

## Testing Guidelines
- Use Rust `#[test]` and `#[tokio::test]` (see `cli/src/command.rs`).
- Name tests `test_*` with clear intent; keep tests hermetic and fast.
- Run locally with `make test`.

## Commit & Pull Request Guidelines
- Commits: concise, imperative; prefixes like `feat:`, `fix:`, `chore:`, `refactor:`, `docs:`, `test:`. Include rationale for non-trivial changes.
- PRs: describe changes and why, link issues, include screenshots/logs for CLI or UX-affecting changes, and pass `make format lint test`.

## Security & Configuration Tips
- Review `script/install.sh` before running; it may need elevated privileges.
- User config: `~/.vorpal/Vorpal.toml`; project config at repo root.
- For reproducible/offline builds, vendor crates with `make vendor` and prefer `TARGET=release`.
