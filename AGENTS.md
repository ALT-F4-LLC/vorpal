# Repository Guidelines

## Project Structure & Modules
- `cli/`: Rust CLI (`vorpal`) with subcommands and tests.
- `config/`: Companion binary (`vorpal-config`) for local config helpers.
- `sdk/rust/`: Rust SDK crate; source of `.proto` files in `sdk/rust/api`.
- `sdk/go/`: Go SDK (`module github.com/ALT-F4-LLC/vorpal/sdk/go`), generated protobufs in `pkg/api`, sample CLI in `cmd/vorpal`.
- `script/`: Dev/bootstrap scripts (see `dev.sh`, `install.sh`).
- `terraform/`: Infra examples/templates.
- `makefile`: Common tasks; CI mirrors these.

## Build, Test, and Development
- Rust: `./script/dev.sh cargo build` (preferred dev env) or `make`/`make build`.
- Checks: `make check`, `make test`, `make format`, `make lint` (deny warnings).
- Package: `make dist` outputs to `./dist/`.
- Services: `make vorpal-start`; Vorpal-in-Vorpal: `make vorpal`.
- Go SDK: `pushd sdk/go && go build ./... && go test ./... && popd`.
- Protobufs (Go): `make generate` regenerates `sdk/go/pkg/api` from `sdk/rust/api`.

## Coding Style & Naming
- Rust 2021, toolchain pinned in `rust-toolchain.toml` (includes `clippy`, `rustfmt`).
- Formatting: `rustfmt` (4-space indent, default rustfmt rules). Run `make format`.
- Linting: `clippy` must pass with no warnings (`make lint`).
- Crate naming: `vorpal-*`; binaries defined via `[[bin]]` in `Cargo.toml`.
- Modules/tests: co-locate `mod tests` with `#[cfg(test)]` in source files.

## Testing Guidelines
- Unit tests live next to sources (e.g., `cli/src/...`), use `#[test]` and async where needed.
- Run all tests: `make test` or `./script/dev.sh cargo test`.
- Keep tests hermetic; prefer temp dirs (`tempfile`) and avoid network.
- Add tests for new commands and config merging (see `cli/src/command.rs` tests for patterns).
 - Cross-SDK parity: verify Go vs Rust artifact configs with `vorpal artifact make --config "Vorpal.go.toml" "vorpal" .` (CI enforces parity across multiple artifacts).

## Commit & Pull Requests
- Commit style: Conventional Commits recommended (`feat:`, `fix:`, `chore:`, `refactor:`, `test:`).
- PRs must: pass `make format`, `make lint`, and `make test` in CI; include a clear description, linked issues, and CLI output/screenshots where meaningful.
- Avoid large mixed changes; one topic per PR. Update docs when behavior or flags change.

## Security & Configuration
- `script/install.sh` touches privileged paths; may require `sudo`—review before running.
- Local dev uses `./script/dev.sh` and optional `direnv` (`direnv allow`) to manage toolchains.
- CI packages artifacts for multiple architectures; do not commit secrets. AWS creds are injected by CI only.
