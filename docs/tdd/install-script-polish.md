---
project: vorpal
maturity: draft
last_updated: 2026-03-20
updated_by: "@staff-engineer"
scope: "Rewrite install.sh for production-quality installer with polished UX across macOS and Linux"
owner: "@staff-engineer"
dependencies:
  - ../ux/install-script.md
---

# Technical Design Document: Installer Script Polish

## 1. Problem Statement

The current `script/install.sh` (~157 lines) is a minimal installer that works but provides a bare-bones experience unsuitable for a broad public audience. Specific issues:

1. **No visual feedback** -- no colors, symbols, progress indication, or banner. The user sees sparse `|>` prefixed lines with no clear phase progression.
2. **No version selection** -- hardcoded to `nightly`. No `--version` flag or `latest` channel.
3. **No upgrade flow** -- existing installation is nuked (`rm -rf`) rather than selectively upgraded, destroying TLS keys and cached data.
4. **No uninstall** -- users have no guided way to remove Vorpal.
5. **No PATH configuration** -- after install, `vorpal` is not on PATH without manual shell config.
6. **No error recovery** -- `set -euo pipefail` exits immediately on any failure with no actionable messaging.
7. **Deprecated macOS service commands** -- uses `launchctl load/unload` instead of modern `bootstrap/bootout`.
8. **System-level systemd** -- requires root for service management; inconsistent with macOS LaunchAgent (user-level).
9. **No CI/headless mode detection** -- `read` prompts can hang when stdin is piped (the `curl | bash` pattern).
10. **No signal handling / cleanup** -- partial downloads leave artifacts on interrupt.

### Why Now

Vorpal is approaching broader adoption. The installer is the first interaction -- it sets expectations for the entire tool. A polished installer reduces support burden and increases successful onboarding.

### Acceptance Criteria

| # | Criterion | Verification |
|---|---|---|
| AC-1 | Fresh install completes in <60s on clean macOS and Linux machines | Timed CI runs across the 4-platform matrix |
| AC-2 | `vorpal --version` works in a new shell immediately after install | Automated: open subshell, run command |
| AC-3 | Services are running after install | `launchctl print` / `systemctl --user is-active` returns success |
| AC-4 | Summary shows version, next-step commands, and links | Visual inspection + grep output |
| AC-5 | `--version nightly`, `--version latest`, `--version v0.1.0-alpha.0` all resolve correctly | Test each against real GitHub releases |
| AC-6 | Upgrade preserves `/var/lib/vorpal/key/` and `/var/lib/vorpal/store/` | Install, create keys, upgrade, verify keys persist |
| AC-7 | `--uninstall` removes all artifacts (binary, dirs, service, PATH entries) | Run uninstall, verify no leftovers |
| AC-8 | Non-interactive mode (`CI=true`, `-y`, `VORPAL_NONINTERACTIVE=1`, piped stdin) produces zero prompts | Run with each trigger, verify no stdin reads |
| AC-9 | `NO_COLOR=1` suppresses all ANSI codes; non-TTY suppresses banner and spinners | Pipe output through `cat -v`, verify no escape sequences |
| AC-10 | Every error message includes what happened, why, and what to do | Trigger each failure path, verify message structure |
| AC-11 | Ctrl+C during install cleans up temp files | Interrupt mid-download, verify no temp artifacts |
| AC-12 | Script runs on bash 3.2+ (macOS default) | Test on macOS with system bash (`/bin/bash --version` = 3.2) |
| AC-13 | PATH is configured for all detected shells (bash, zsh, fish) with idempotent guards | Run twice, verify no duplicate entries |

---

## 2. Context & Prior Art

### Current Installer Analysis

The current `script/install.sh` has this flow:

1. Parse `-y`/`--yes`/`--help` flags
2. Prompt for sudo confirmation (blocks on `curl | bash`)
3. If `~/.vorpal` exists, prompt to replace (destructive: `rm -rf`)
4. `mkdir -p ~/.vorpal/bin`
5. `curl | tar` directly into final location (no atomic download)
6. `sudo mkdir` for `/var/lib/vorpal/*`
7. `vorpal system keys generate`
8. Platform-specific service setup (launchctl load / systemd system-level)
9. Print "installed and started"

Key findings from codebase exploration:

- **Release artifacts**: Named `vorpal-{arch}-{os}.tar.gz` (e.g., `vorpal-aarch64-darwin.tar.gz`). Four platform combinations: `aarch64-darwin`, `x86_64-darwin`, `aarch64-linux`, `x86_64-linux`. Published via `softprops/action-gh-release` in `.github/workflows/vorpal.yaml`.
- **Nightly releases**: The `vorpal-nightly.yaml` workflow deletes and recreates the `nightly` tag on `main` daily. This means the nightly release URL is stable.
- **No SHA-256 checksums** published alongside tarballs. Build provenance attestation exists via `actions/attest-build-provenance` but is not consumable from a shell script.
- **Key generation** (`cli/src/command/system/keys.rs`): Idempotent -- each key file is only generated if `!path.exists()`. Generates: CA key, CA cert, service key, service public key, service cert, service secret. All stored under `/var/lib/vorpal/key/`.
- **Service command**: `vorpal system services start` is the entry point. Listens on Unix domain socket at `/var/lib/vorpal/vorpal.sock` by default (configurable via `VORPAL_SOCKET_PATH`).
- **Directory structure**: `/var/lib/vorpal/{key,log,sandbox,store}` with `store/artifact/{alias,archive,config,output}`.
- **Arch detection**: Both makefile and current installer use `uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/'`.

### Prior Art

| Installer | Strengths | Weaknesses |
|---|---|---|
| **rustup** | Multi-channel version selection, component management, shell PATH config, upgrade/uninstall, robust error handling | Complex (1500+ lines), heavy on prompts |
| **Homebrew** | Iconic banner, clear prerequisites check, great error messages, idempotent | macOS-focused, long install time |
| **Deno** | Simple `curl | sh`, fast, minimal prompts, good PATH hint | No service management, no upgrade flow |
| **Starship** | Beautiful spinners, multi-shell PATH config, platform detection | No services, no system dirs |

The Vorpal installer is most similar to Deno's in scope (single binary) but with the added complexity of system directories (sudo), TLS keys, and background services. The visual approach should follow Starship's polish with Deno's simplicity.

---

## 3. Alternatives Considered

### Alternative A: Single Monolithic Script (Recommended)

Keep everything in one `install.sh` file, organized into well-defined functions. The current pattern, but with proper structure.

**Strengths:** Single-file download, easy to audit, no dependencies, simple `curl | bash`.
**Weaknesses:** Will grow to ~600-800 lines; harder to test individual functions in isolation.

### Alternative B: Multi-File Script with Loader

Split into `install.sh` (entry point) + `lib/*.sh` (modules). The entry point downloads and sources the library files.

**Strengths:** Better code organization, easier unit testing.
**Weaknesses:** Adds complexity to the download step (must fetch multiple files or a tarball of scripts), breaks `curl | bash` simplicity, harder to audit.

### Alternative C: Compiled Installer Binary

Write the installer in Rust/Go and distribute as a separate binary.

**Strengths:** Full language features (error handling, testing, concurrency), better cross-platform abstraction.
**Weaknesses:** Chicken-and-egg problem (need something to install the installer), much higher maintenance burden, breaks `curl | bash` convention.

### Recommendation

**Alternative A** -- single file. The installer is fundamentally a sequential pipeline of ~9 phases. The complexity is in the UX polish (colors, spinners, error messages), not in architectural structure. A well-organized single file with clear function boundaries achieves the goal without overengineering. The 600-800 line estimate is manageable for a bash script when functions are well-named and the structure mirrors the UX spec's phase model.

---

## 4. Architecture & System Design

### Script Structure

The script is organized into four layers, declared top-to-bottom:

```
[1] Constants & Configuration
[2] Utility Functions (output, platform, environment)
[3] Phase Functions (one per install phase)
[4] Orchestration (main)
```

#### Layer 1: Constants & Configuration

```
VORPAL_VERSION       -- default "nightly", overridden by --version / VORPAL_VERSION env
VORPAL_INSTALL_DIR   -- "$HOME/.vorpal"
VORPAL_SYSTEM_DIR    -- "/var/lib/vorpal"
VORPAL_REPO          -- "ALT-F4-LLC/vorpal"
VORPAL_GITHUB_URL    -- "https://github.com/$VORPAL_REPO"
```

Flags parsed at the top:
- `-y` / `--yes`: non-interactive
- `-v` / `--version <ver>`: version selection
- `--no-service`: skip service phase
- `--no-path`: skip PATH configuration
- `--uninstall`: run uninstall flow
- `-h` / `--help`: print help

#### Layer 2: Utility Functions

**Output system:**

| Function | Purpose |
|---|---|
| `has_color` | Returns 0 if color output should be used. Checks: `NO_COLOR` env, stdout is TTY. |
| `has_unicode` | Returns 0 if Unicode glyphs should be used. Checks: `printf '\xe2\x9c\x94'` width test, falls back if not TTY or `NO_COLOR`. |
| `is_interactive` | Returns 0 if interactive mode. Checks (any = non-interactive): `-y` flag, `VORPAL_NONINTERACTIVE=1`, `CI=true`, `! [ -t 0 ]`. |
| `print_banner` | ASCII art banner with version/platform. Suppressed when non-TTY stdout. |
| `print_header` | Bold cyan section header with blank line prefix. |
| `print_step` | Step line with spinner (interactive) or static prefix (non-interactive). |
| `print_success` | Replace spinner with green checkmark + completion text. |
| `print_warning` | Yellow warning symbol + text. |
| `print_error` | Red error with structured what/why/what-to-do format. |
| `spin` | Background spinner using braille characters at 80ms intervals. Manages spinner PID for cleanup. |
| `spin_stop` | Kill spinner process, replace with final status. |

**Spinner implementation detail:** The spinner runs as a background subshell writing `\r`-overwritten lines to stderr (so it does not interfere with stdout piping). The main script stores the spinner PID and kills it before printing the final status line. When stdout is not a TTY, `spin` is a no-op and `print_step` just prints a static line.

**Platform utilities:**

| Function | Purpose |
|---|---|
| `detect_arch` | `uname -m`, normalize `arm64` to `aarch64`. Set `ARCH` and `ARCH_LABEL`. |
| `detect_os` | `uname -s`, normalize to lowercase. Set `OS` and `OS_LABEL`. |
| `detect_platform` | Calls both, validates supported combinations, sets `PLATFORM` string. |

#### Layer 3: Phase Functions

Each function corresponds to a UX spec phase. They are called sequentially by `main`. Each function:
- Prints its own header and step lines
- Returns 0 on success, non-zero on failure
- Sets any state needed by subsequent phases via global variables (acceptable in a linear bash script)

| Function | UX Phase | Notes |
|---|---|---|
| `check_prerequisites` | Phase 1 | Checks `curl`, `tar` via `command -v` |
| `resolve_version` | Phase 2 | Maps input to download URL; validates via HTTP HEAD |
| `download_binary` | Phase 4 | Temp dir download, extract, verify, atomic `mv` |
| `setup_system_dirs` | Phase 5 | `sudo mkdir` with ownership; skips if exists and owned correctly |
| `generate_keys` | Phase 6 | Invokes `vorpal system keys generate`; idempotent |
| `install_service` | Phase 7 | Dispatches to `install_service_macos` or `install_service_linux` |
| `install_service_macos` | Phase 7 | Write plist, `launchctl bootout` (ignore errors), `launchctl bootstrap` |
| `install_service_linux` | Phase 7 | Write systemd user unit, `systemctl --user daemon-reload`, enable, start |
| `verify_service` | Phase 7 | Poll up to 5s for service active state |
| `configure_path` | Phase 8 | Detect shells, write guarded PATH entries |
| `print_summary` | Phase 9 | Success message, quickstart commands, links |
| `run_uninstall` | Uninstall | Stop service, remove files, remove PATH entries |
| `handle_existing` | Phase 4 | Detect existing install, prompt for upgrade/reinstall/cancel |

#### Layer 4: Orchestration

```
main() {
    parse_args "$@"
    setup_trap
    detect_platform

    if [ "$UNINSTALL" = 1 ]; then
        run_uninstall
        exit 0
    fi

    print_banner
    check_prerequisites
    resolve_version
    handle_existing        # only if ~/.vorpal/bin/vorpal exists
    download_binary
    setup_system_dirs
    generate_keys

    if [ "$NO_SERVICE" != 1 ]; then
        install_service
        verify_service
    fi

    if [ "$NO_PATH" != 1 ]; then
        configure_path
    fi

    print_summary
}
```

### Shell Compatibility Strategy

**Target:** bash 3.2+ (macOS ships 3.2.57 due to GPLv3 licensing of bash 4+).

**Allowed constructs:**
- `[[ ]]` for conditionals (available since bash 2.02)
- `local` variables in functions
- `$(command)` substitution
- `${var:-default}` parameter expansion
- Here-docs and here-strings
- Arrays (indexed, not associative -- associative arrays require bash 4+)
- `printf` for formatted output (preferred over `echo -e` for portability)
- `trap` for signal handling

**Explicitly avoided:**
- Associative arrays (`declare -A`) -- bash 4+ only
- `mapfile` / `readarray` -- bash 4+ only
- `|&` pipe stderr -- bash 4+ only
- `${var,,}` lowercase expansion -- bash 4+ only (use `tr '[:upper:]' '[:lower:]'` instead)
- `>&` redirect shorthand in some forms
- Nameref variables (`declare -n`) -- bash 4.3+ only

**Shebang:** `#!/bin/bash` (not `#!/usr/bin/env bash` -- macOS has `/bin/bash`, and `env` resolution can pick up brew-installed bash unexpectedly in some environments; however, either works). The script also includes `set -euo pipefail`.

**Note on `set -e`:** Functions that intentionally allow non-zero returns (e.g., `launchctl bootout` on first install) must use explicit `|| true` or conditional checks to prevent premature exit.

---

## 5. Data Models & Storage

No new data models. The installer creates and manages these filesystem paths:

| Path | Owner | Purpose | Created by |
|---|---|---|---|
| `~/.vorpal/bin/vorpal` | user | CLI binary | `download_binary` |
| `/var/lib/vorpal/key/*` | user (via chown) | TLS keys (CA, service certs, secret) | `generate_keys` (via `vorpal system keys generate`) |
| `/var/lib/vorpal/log/` | user (via chown) | Service logs | `setup_system_dirs` |
| `/var/lib/vorpal/sandbox/` | user (via chown) | Build sandboxes | `setup_system_dirs` |
| `/var/lib/vorpal/store/artifact/{alias,archive,config,output}` | user (via chown) | Artifact store | `setup_system_dirs` |
| `~/Library/LaunchAgents/com.altf4llc.vorpal.plist` | user | macOS service config | `install_service_macos` |
| `~/.config/systemd/user/vorpal.service` | user | Linux service config | `install_service_linux` |
| Shell rc files (`.zshrc`, `.bashrc`, etc.) | user | PATH configuration | `configure_path` |

### Upgrade Semantics

The upgrade path replaces ONLY `~/.vorpal/bin/vorpal`. Everything else is preserved:
- `/var/lib/vorpal/key/*` -- keys are idempotent (code checks `!path.exists()`)
- `/var/lib/vorpal/store/*` -- cached artifacts survive
- Service configs -- rewritten (same content), service restarted
- PATH entries -- idempotent (guarded by comment marker check)

### Uninstall Semantics

Complete removal in this order:
1. Stop and remove service (launchctl bootout / systemctl --user stop + disable)
2. Remove service config file
3. Remove `~/.vorpal/` (binary directory)
4. Remove `/var/lib/vorpal/` (requires sudo)
5. Remove PATH entries from shell rc files (find and delete the `# Vorpal` marker block)

---

## 6. API Contracts

### GitHub Release URL Format

```
https://github.com/ALT-F4-LLC/vorpal/releases/download/{tag}/vorpal-{arch}-{os}.tar.gz
```

Where:
- `{tag}`: `nightly`, or a version tag like `v0.1.0-alpha.0`
- `{arch}`: `aarch64` or `x86_64`
- `{os}`: `darwin` or `linux`

### GitHub API for `latest` Resolution

```
GET https://api.github.com/repos/ALT-F4-LLC/vorpal/releases/latest
```

Returns JSON with `tag_name` field. Requires no authentication for public repos. Subject to rate limiting (60 req/hr unauthenticated).

### Version Validation

Use HTTP HEAD request against the download URL to validate version existence without downloading:

```bash
curl -fsSI -o /dev/null "https://github.com/.../vorpal-{arch}-{os}.tar.gz"
```

404 response indicates invalid version. This is more reliable than querying the releases API because it checks the specific platform artifact, not just the tag.

**Design decision:** Version validation via HEAD request is preferred over a separate API call because it verifies both the tag AND the platform artifact exist in a single request. The GitHub releases download endpoint follows redirects to the CDN, so the HEAD request also validates CDN reachability.

---

## 7. Migration & Rollout

### Current-to-Proposed Path

The new installer is a drop-in replacement for the existing `script/install.sh`. The invocation pattern remains:

```bash
curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash
```

**Backward compatibility considerations:**
- Users with existing installations will see the upgrade flow (new behavior) instead of the destructive `rm -rf` (old behavior). This is strictly better.
- The `-y`/`--yes` flag behavior is preserved.
- `CI=true` and `VORPAL_NONINTERACTIVE=1` continue to work.

### Rollout Plan

1. **Merge to `main`** -- the raw GitHub URL immediately serves the new installer
2. **No flag day** -- the new installer handles both fresh and existing installations
3. **No versioning of the installer itself** -- it downloads a versioned binary; the installer is unversioned

### Rollback Plan

If critical issues are found, revert the commit on `main`. The raw GitHub URL will immediately serve the old installer. Existing installations are not affected (the installer only runs when explicitly invoked).

---

## 8. Risks & Open Questions

### Known Risks

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| bash 3.2 compatibility gap | Medium | High -- macOS users can't install | Test on macOS system bash in CI; avoid bash 4+ features per the compatibility list above |
| Spinner misbehavior on non-standard terminals | Low | Low -- cosmetic only | Spinner is suppressed for non-TTY; ASCII fallback for degraded terminals |
| `launchctl bootstrap` behavioral differences across macOS versions | Low | Medium -- service fails to start | Test on macOS 12+ (CI runners); provide manual restart instructions in error message |
| GitHub API rate limiting for `--version latest` | Medium | Low -- only affects `latest` channel | Fall back to nightly with a warning; document that `GITHUB_TOKEN` env var can be used for higher limits |
| systemd user services don't start on boot without `loginctl enable-linger` | Medium | Medium -- service doesn't survive reboot on Linux | Document in post-install output; consider running `loginctl enable-linger` with user consent |
| `curl | bash` and stdin | Addressed | Was High | Non-interactive detection includes `! [ -t 0 ]` which correctly identifies piped stdin |

### Open Questions (Resolved for v1)

1. **SHA-256 checksums:** NOT available in v1. The release pipeline does not publish `.sha256` sidecar files. The download verification step (`vorpal --version`) serves as a basic integrity check. **Future enhancement:** Add `vorpal-{arch}-{os}.tar.gz.sha256` to the release workflow.

2. **`get.vorpal.sh` domain:** NOT available for v1. The installer URL remains the raw GitHub URL. **Future enhancement:** Set up a redirect from `get.vorpal.sh` to the raw GitHub URL.

3. **Component selection:** Deferred to v2 per the TODO in the current script. The installer installs all services.

4. **systemd unit level:** Use **user-level** (`systemctl --user`, unit at `~/.config/systemd/user/vorpal.service`). This is consistent with the macOS LaunchAgent pattern (user-level, no root for service management), avoids sudo for service operations, and means the service runs when the user is logged in -- appropriate for a development tool. The tradeoff (service doesn't run without a login session) is acceptable because Vorpal is a development tool, not a production server.

### New Open Question

5. **`loginctl enable-linger`:** On Linux with systemd user services, the service only runs when the user has an active login session. `enable-linger` makes user services persist across sessions. Should the installer run this automatically? **v1 decision:** Do NOT run it automatically. Print a note in the summary if on Linux: "To keep services running after logout: `loginctl enable-linger`". This avoids surprising system-level changes.

---

## 9. Testing Strategy

### Test Levels

| Level | What | How | Where |
|---|---|---|---|
| **Static analysis** | shellcheck | `shellcheck script/install.sh` | CI on every PR |
| **Unit-like tests** | Individual function behavior (platform detection, version resolution, PATH guarding) | Extract functions into a sourceable form; test with bats-core or direct bash assertions | CI |
| **Integration tests** | Full install/upgrade/uninstall on clean systems | Docker containers (Linux) + macOS CI runners | CI matrix: `ubuntu-latest`, `ubuntu-latest-arm64`, `macos-latest`, `macos-latest-large` |
| **Behavioral tests** | Non-interactive mode, NO_COLOR, signal handling, error messages | Scripted test harness that runs the installer with various env/flag combinations | CI |

### Key Scenarios

1. **Fresh install (interactive, macOS Apple Silicon)** -- happy path
2. **Fresh install (non-interactive, Linux x86_64)** -- CI mode
3. **Upgrade (existing install, preserve keys)** -- verify key survival
4. **Uninstall (complete removal)** -- verify no artifacts
5. **Invalid version** -- verify error message format
6. **No sudo** -- verify graceful degradation
7. **Ctrl+C mid-download** -- verify temp cleanup
8. **NO_COLOR=1** -- verify no ANSI codes in output
9. **Non-TTY stdout** -- verify no banner, no spinners
10. **bash 3.2** -- verify no syntax errors on macOS system bash
11. **Multiple shells present** -- verify PATH configured for all
12. **Re-run after successful install** -- verify idempotent PATH, detect existing install

### shellcheck Configuration

The script should pass `shellcheck` with no warnings. Known necessary exceptions (if any) are annotated inline with `# shellcheck disable=SC####` and a comment explaining why.

---

## 10. Observability & Operational Readiness

### Installer Observability

The installer itself has no telemetry (per the UX spec privacy section). Observability comes from:

1. **Structured output** -- every phase produces a success/failure line. In CI (`VORPAL_NONINTERACTIVE=1`), this output is parseable for automated success/failure detection.
2. **Exit codes** -- `0` for success, `1` for recoverable failure, `130` for user interrupt (SIGINT).
3. **Error message structure** -- every error includes what/why/what-to-do, making GitHub issue reports self-documenting.

### Service Health After Install

The `verify_service` function polls for up to 5 seconds after starting the service:
- **macOS:** `launchctl print gui/$(id -u)/com.altf4llc.vorpal` -- check for "state = running"
- **Linux:** `systemctl --user is-active vorpal.service` -- check for "active"

If verification fails, the installer prints diagnostic commands (log paths, restart commands) but exits 0 -- the binary is installed and functional even if the service failed to start.

### Diagnosability

| Scenario | Diagnostic |
|---|---|
| Service won't start | `cat /var/lib/vorpal/log/services.log` (macOS) or `journalctl --user -u vorpal.service` (Linux) |
| Binary not found after install | Check PATH config, check `~/.vorpal/bin/vorpal` exists |
| Keys not generated | Check `/var/lib/vorpal/key/` permissions, re-run `vorpal system keys generate` |
| Download fails | URL printed in error message; user can test manually with `curl -fSL` |

---

## 11. Implementation Phases

### Phase 1: Core Infrastructure (S)

**Scope:** Script skeleton, argument parsing, output utilities, platform detection.

**Deliverables:**
- Shebang, `set -euo pipefail`, constants
- `parse_args` with all flags (`-y`, `-v`, `--no-service`, `--no-path`, `--uninstall`, `-h`)
- `has_color`, `has_unicode`, `is_interactive`
- `detect_arch`, `detect_os`, `detect_platform`
- `print_banner`, `print_header`, `print_step`, `print_success`, `print_warning`, `print_error`
- `cleanup` trap handler with temp dir removal
- Signal handling: SIGINT (exit 130), SIGTERM (exit 130), SIGHUP (ignored)

**Dependencies:** None.

**Testable independently:** Yes -- the script can be sourced and functions called in isolation. `shellcheck` should pass.

### Phase 2: Download & Version Resolution (S)

**Scope:** Version resolution, download, extraction, verification.

**Deliverables:**
- `resolve_version` -- map `nightly` / `latest` / exact tag to download URL; validate via HEAD request
- `download_binary` -- download to temp dir, extract, verify via `vorpal --version`, atomic `mv`
- `handle_existing` -- detect existing install, prompt for upgrade/reinstall/cancel (auto-upgrade in non-interactive)
- Error messages for: invalid version (404), download failure, binary verification failure

**Dependencies:** Phase 1 (output utilities, platform detection).

### Phase 3: System Setup & Key Generation (S)

**Scope:** System directory creation, key generation, sudo handling.

**Deliverables:**
- `setup_system_dirs` -- create `/var/lib/vorpal/` tree, chown to current user; skip if exists with correct ownership
- `generate_keys` -- invoke `vorpal system keys generate`; idempotent
- Sudo pre-announcement ("requires sudo") before the sudo prompt
- Error messages for: sudo failure, key generation failure

**Dependencies:** Phase 2 (binary must be downloaded before key generation).

### Phase 4: Service Management (M)

**Scope:** Platform-specific service installation, verification.

**Deliverables:**
- `install_service_macos` -- write plist, `launchctl bootout` (ignore errors), `launchctl bootstrap`
- `install_service_linux` -- write systemd user unit to `~/.config/systemd/user/vorpal.service`, `systemctl --user daemon-reload`, enable, start
- `verify_service` -- poll for up to 5s, report success/failure
- `--no-service` flag handling
- Error messages for: service start failure with diagnostic commands

**Dependencies:** Phase 3 (system dirs and keys must exist for service to start).

**Complexity note:** This is Medium because of the two platform paths, the switch from system to user systemd, and the switch from deprecated to modern launchctl commands. Each requires careful testing on its target platform.

### Phase 5: Shell PATH Configuration (S)

**Scope:** Multi-shell PATH configuration with idempotent guards.

**Deliverables:**
- `configure_path` -- detect shells via `$SHELL` + existence of rc files
- Write guarded PATH entry for bash (`.bashrc` on Linux, `.bash_profile` on macOS), zsh (`.zshrc`), fish (`config.fish`)
- Comment marker: `# Vorpal (https://github.com/ALT-F4-LLC/vorpal)`
- Skip if marker already present
- "source" hint for the default shell
- `--no-path` flag handling
- Unrecognized shell fallback with manual instructions

**Dependencies:** Phase 1 (output utilities).

### Phase 6: Summary, Upgrade & Uninstall (S)

**Scope:** Summary output, upgrade display, uninstall flow.

**Deliverables:**
- `print_summary` -- divider, success message, quickstart commands, links
- Upgrade summary variant (version comparison, "keys preserved", "services restarted")
- `run_uninstall` -- confirmation prompt (default N), stop service, remove files, remove PATH entries
- Non-interactive uninstall requires explicit `--yes`
- `loginctl enable-linger` note for Linux

**Dependencies:** All other phases (summary needs to know what was done).

### Phase 7: Spinner Animation (S)

**Scope:** Background spinner for long-running steps.

**Deliverables:**
- `spin` -- background subshell with braille character cycle at 80ms
- `spin_stop` -- kill spinner, print final status
- Graceful degradation: no-op when not a TTY
- ASCII fallback: `- \ | /` when Unicode not available

**Dependencies:** Phase 1 (output utilities, TTY detection).

**Note:** This phase is separated because the spinner is the most complex output utility and can be developed/tested independently. All other phases work without it (they print static lines until the spinner is integrated).

### Dependency Graph

```
Phase 1 (Infrastructure)
  |
  +-- Phase 2 (Download)
  |     |
  |     +-- Phase 3 (System Setup)
  |           |
  |           +-- Phase 4 (Services)
  |
  +-- Phase 5 (PATH Config)
  |
  +-- Phase 7 (Spinner)
  |
  +-- Phase 6 (Summary/Upgrade/Uninstall) -- depends on all above
```

Phases 2, 5, and 7 can be developed in parallel after Phase 1. Phase 6 integrates everything.

### Estimated Total Size

~600-800 lines of bash. The current script is 157 lines with minimal functionality. The UX spec's component breakdown lists 18 functions, each averaging 30-40 lines including error handling and output formatting.
