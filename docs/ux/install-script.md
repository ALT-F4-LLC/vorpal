---
project: vorpal
maturity: draft
last_updated: 2026-03-20
updated_by: ux-designer
scope: installer-script
owner: ux-designer
dependencies:
  - GitHub release pipeline (vorpal.yaml)
  - vorpal system keys generate command
  - vorpal system services start command
  - LaunchAgent / systemd service templates
---

# Vorpal Installer UX Specification

## 1. Overview

### Surface Type

Shell script (bash), invoked via `curl | bash` pipe or direct execution. This is a CLI
installation experience -- the user's first interaction with Vorpal.

### Users

| Attribute | Description |
|---|---|
| **Skill level** | Intermediate to advanced developers comfortable with terminal workflows |
| **Context** | Installing a build system; likely evaluating Vorpal for the first time or onboarding onto a team that uses it |
| **Frequency** | Once per machine (install), occasionally (upgrade), rarely (uninstall) |
| **Environment** | Local development machines (macOS, Linux); CI runners (headless, non-interactive) |

### Key Workflows (prioritized)

1. **Fresh install** -- Download binary, configure system directories, generate TLS keys, start services, configure PATH
2. **Upgrade** -- Replace existing binary with a newer version, restart services, preserve configuration
3. **Uninstall** -- Remove binary, services, system directories, and shell configuration
4. **Version selection** -- Install a specific release (nightly, latest tagged, named version)
5. **CI/headless install** -- Non-interactive mode that skips prompts and produces machine-parseable output

### Success Criteria

| # | Criterion | Testable condition |
|---|---|---|
| SC-1 | User can install Vorpal in under 60 seconds on a clean machine | Timer from curl to "ready" message |
| SC-2 | `vorpal --version` works immediately after install without manual PATH changes | Run in a new shell after install |
| SC-3 | Services are running and healthy after install | `vorpal system services start` has a running process (launchctl/systemctl status) |
| SC-4 | User understands what happened and what to do next | Final output includes version, next-step commands |
| SC-5 | Errors are actionable -- user can self-resolve or report effectively | Every error message includes: what happened, why, what to do |
| SC-6 | CI mode produces zero interactive prompts and exits with correct codes | Run with `CI=true`, verify no stdin reads, verify exit codes |
| SC-7 | Upgrade preserves existing keys and configuration | Keys in `/var/lib/vorpal/key/` survive upgrade |
| SC-8 | Uninstall leaves no artifacts on the system | No `~/.vorpal`, no `/var/lib/vorpal`, no service configs, no PATH entries |

### Success Metrics

| Metric | Target | Measurement method |
|---|---|---|
| Install success rate | >95% of attempts complete without error | Recommend telemetry ping (opt-in) or track GitHub issue volume |
| Time to first build | <5 min from install start to `vorpal build` success | Documented in quickstart; measurable in user testing |
| Support ticket rate | <5% of installs result in a support request | Track GitHub issues tagged "installer" |

---

## 2. Information Architecture

### User-Facing Data Model

The installer operates on these concepts, presented in this order:

| Concept | User-facing name | Description |
|---|---|---|
| **Platform** | "your system" | Architecture + OS combination (e.g., "macOS Apple Silicon") |
| **Version** | "version" | Release channel or tag: `nightly`, `latest`, or a specific tag like `v0.1.0-alpha.0` |
| **Binary** | "Vorpal" or "the vorpal binary" | The single CLI executable |
| **System directories** | "system storage" | `/var/lib/vorpal/` tree for keys, logs, sandbox, and artifact store |
| **TLS keys** | "security keys" | CA and service certificates for local gRPC communication |
| **Services** | "background services" | LaunchAgent (macOS) or systemd unit (Linux) running `vorpal system services start` |
| **Shell configuration** | "PATH setup" | Additions to shell rc files to put `vorpal` on PATH |

### Information Hierarchy

The installer presents information in a strict narrative order. Each phase announces what it
will do, does it, and confirms success before moving to the next phase. The user should never
wonder "what is it doing now?"

```
1. Banner          -- Identity + version
2. Platform check  -- What system we detected
3. Prerequisites   -- What tools we need (curl, tar)
4. Download        -- Fetching the binary
5. System setup    -- Creating directories (sudo context)
6. Security        -- Generating TLS keys
7. Services        -- Installing and starting background services
8. Shell setup     -- Configuring PATH
9. Summary         -- What was installed, what to do next
```

---

## 3. Layout & Structure

### Visual Design System

#### Color Palette (Semantic)

All colors are applied via ANSI escape codes. The installer respects `NO_COLOR` (see section 7).

| Role | ANSI | Usage |
|---|---|---|
| **Brand / accent** | Bold cyan (36;1) | Banner text, section headers, the word "Vorpal" in key moments |
| **Success** | Green (32) | Checkmarks, completion confirmations |
| **Warning** | Yellow (33) | Non-fatal issues, prompts requiring attention |
| **Error** | Bold red (31;1) | Fatal errors, failed steps |
| **Muted / secondary** | Dim (2) | Elapsed time, file paths, technical details |
| **Default** | Reset (0) | Body text, descriptions |

#### Symbol Set

| Symbol | Unicode | Fallback (ASCII) | Meaning |
|---|---|---|---|
| Success | `[check]` (U+2714) | `[ok]` | Step completed |
| Failure | `[cross]` (U+2718) | `[FAIL]` | Step failed |
| Arrow | `[arrow]` (U+2192) | `->` | "Next" or "leads to" |
| Bullet | `[bullet]` (U+2022) | `-` | List item |
| Spinner | Braille pattern cycle | `- \ \| /` | In-progress operation |

Unicode detection: Test `printf '\xe2\x9c\x94'` output width. If the terminal reports width 1,
use Unicode glyphs. Otherwise, fall back to ASCII. In `NO_COLOR` mode or when stdout is not a
TTY, always use ASCII fallback.

#### Typography Hierarchy

All text uses the terminal's monospace font. Hierarchy is created through:

1. **Section headers**: Bold + cyan, preceded by a blank line
2. **Step labels**: Bold white, prefixed with a spinner (in-progress) or status symbol (done)
3. **Detail text**: Default weight, indented 2 spaces under the step
4. **Muted metadata**: Dim text for paths, timing, versions

#### Progress Indication

Each step shows a spinner while in progress, replaced by a success/failure symbol on completion:

```
  [spinner] Downloading Vorpal nightly...
```

becomes:

```
  [check] Downloaded Vorpal nightly (4.2 MB, 1.2s)
```

Spinner implementation: Cycle through braille patterns (`[braille-1]` `[braille-2]` `[braille-3]` `[braille-4]` `[braille-5]` `[braille-6]` `[braille-7]` `[braille-8]`) at 80ms intervals using
`\r` carriage return to overwrite the line. When stdout is not a TTY (piped/CI), skip spinner
animation entirely and print a single static line per step.

### Banner

```
                           __
 _   _____  _________  ____ _/ /
| | / / __ \/ ___/ __ \/ __ `/ /
| |/ / /_/ / /  / /_/ / /_/ / /
|___/\____/_/  / .___/\__,_/_/
              /_/

  Build system that works as code.

  Version:  nightly (2026-03-20)
  Platform: macOS Apple Silicon (aarch64-darwin)
```

The banner is displayed in bold cyan. "Build system that works as code." is in default weight.
Version and platform lines are in dim text with labels in default weight.

When stdout is not a TTY (piped install), the banner is suppressed entirely. Only structured
step output is shown.

### Full Interactive Session Layout

Below is the complete output layout for a successful fresh install on macOS. Each section is
separated by a single blank line for scanability.

```
[banner as above]

  Checking prerequisites
  [check] curl 8.7.1
  [check] tar (bsdtar) 3.5.3

  Downloading
  [check] Downloaded vorpal nightly (4.2 MB, 1.2s)
  [check] Verified binary (SHA-256)

  Setting up system storage
  [!] Vorpal needs to create /var/lib/vorpal (requires sudo)
  [sudo prompt appears here if needed]
  [check] Created system directories

  Generating security keys
  [check] Generated CA certificate
  [check] Generated service certificate

  Starting services
  [check] Installed LaunchAgent
  [check] Services running

  Configuring shell
  [check] Added ~/.vorpal/bin to PATH in ~/.zshrc
  [!] Open a new terminal or run: source ~/.zshrc

  -------------------------------------------------------

  Vorpal nightly installed successfully.

  Get started:
    mkdir hello-world && cd hello-world
    vorpal init hello-world
    vorpal build hello-world

  Docs:     https://github.com/ALT-F4-LLC/vorpal
  Issues:   https://github.com/ALT-F4-LLC/vorpal/issues
```

---

## 4. Interaction Design

### 4.1 Fresh Install Flow

#### Phase 0: Argument Parsing & Environment Detection

**Supported flags:**

| Flag | Environment var | Description |
|---|---|---|
| `-y`, `--yes` | `VORPAL_NONINTERACTIVE=1` or `CI=true` | Non-interactive mode |
| `-v`, `--version <version>` | `VORPAL_VERSION` | Version to install (default: `nightly`) |
| `--no-service` | `VORPAL_NO_SERVICE=1` | Skip service installation |
| `--no-path` | `VORPAL_NO_PATH=1` | Skip PATH configuration |
| `--uninstall` | -- | Run uninstall flow |
| `-h`, `--help` | -- | Print help and exit |

**Non-interactive detection** (in priority order):
1. `-y` / `--yes` flag
2. `VORPAL_NONINTERACTIVE=1`
3. `CI=true` (standard CI variable)
4. stdin is not a TTY (`! [ -t 0 ]`)

If any of these conditions are true, all prompts are auto-accepted and the banner is suppressed
when stdout is also not a TTY.

Note: The fourth condition (`stdin is not a TTY`) is important because the standard invocation
pattern `curl ... | bash` pipes curl's output to bash's stdin, making stdin not a TTY. Without
this detection, `read` prompts would hang or fail silently.

#### Phase 1: Prerequisites

Check that required tools are available before doing anything destructive.

**Required tools:**
- `curl` -- for downloading the binary
- `tar` -- for extracting the archive

**Check method:** `command -v <tool>` (POSIX-portable, does not invoke the tool).

**On success:** Print each tool with its version and a checkmark.

**On failure:**

```
  [cross] Missing required tools:
    [bullet] curl -- install via your package manager
    [bullet] tar -- install via your package manager

  Install them and re-run the installer.
```

Exit code: 1.

#### Phase 2: Version Resolution

**Input:** `--version` flag value or `VORPAL_VERSION` env var. Defaults to `nightly`.

**Resolution rules:**

| Input | Resolved URL |
|---|---|
| `nightly` | `https://github.com/ALT-F4-LLC/vorpal/releases/download/nightly/vorpal-{arch}-{os}.tar.gz` |
| `latest` | Query GitHub API for latest non-prerelease tag, then use that tag's URL |
| `v0.1.0-alpha.0` (exact tag) | `https://github.com/ALT-F4-LLC/vorpal/releases/download/{tag}/vorpal-{arch}-{os}.tar.gz` |

**On invalid version** (HTTP 404 from GitHub):

```
  [cross] Version "v99.99.99" not found.

  Available channels:
    [bullet] nightly -- latest development build (updated daily)
    [bullet] latest  -- most recent stable release

  Or specify an exact tag: --version v0.1.0-alpha.0
  See all releases: https://github.com/ALT-F4-LLC/vorpal/releases
```

Exit code: 1.

#### Phase 3: Platform Detection

**Architecture mapping:**

| `uname -m` output | Vorpal arch string | User-facing label |
|---|---|---|
| `x86_64` | `x86_64` | "x86_64" |
| `aarch64`, `arm64` | `aarch64` | "Apple Silicon" (macOS) or "ARM64" (Linux) |

**OS mapping:**

| `uname -s` output | Vorpal OS string | User-facing label |
|---|---|---|
| `Darwin` | `darwin` | "macOS" |
| `Linux` | `linux` | "Linux" |

**Unsupported platform:**

```
  [cross] Unsupported platform: FreeBSD x86_64

  Vorpal supports:
    [bullet] macOS (Apple Silicon, Intel)
    [bullet] Linux (x86_64, ARM64)

  Building from source: https://github.com/ALT-F4-LLC/vorpal#contributing
```

Exit code: 1.

#### Phase 4: Download & Verification

1. Create `~/.vorpal/bin/` if it does not exist
2. Download the tarball with `curl -fSL` (fail on HTTP errors, show errors, follow redirects)
3. Show spinner with download progress
4. Extract with `tar xz -C "$HOME/.vorpal/bin"`
5. Verify the binary is executable: `"$HOME/.vorpal/bin/vorpal" --version`

**Download failure:**

```
  [cross] Download failed (HTTP 404)

  Could not download Vorpal nightly for aarch64-darwin.
  URL: https://github.com/ALT-F4-LLC/vorpal/releases/download/nightly/vorpal-aarch64-darwin.tar.gz

  This usually means:
    [bullet] The version does not exist -- check: https://github.com/ALT-F4-LLC/vorpal/releases
    [bullet] Your platform is not supported for this version
    [bullet] GitHub is experiencing an outage -- check: https://www.githubstatus.com
```

**Binary verification failure** (`vorpal --version` returns non-zero):

```
  [cross] Downloaded binary failed verification

  The file was downloaded but does not appear to be a valid Vorpal binary.
  This may indicate a corrupted download or incompatible binary.

  Try again: curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash
  Report:    https://github.com/ALT-F4-LLC/vorpal/issues
```

**Existing installation detected:**

If `~/.vorpal/bin/vorpal` already exists and this is not an explicit upgrade:

```
  [!] Vorpal is already installed (nightly, 2026-03-15)

  Options:
    1) Upgrade to nightly (2026-03-20) [default]
    2) Reinstall nightly
    3) Cancel

  Choice [1]:
```

In non-interactive mode, option 1 (upgrade) is selected automatically.

**Upgrade behavior:** The upgrade path preserves the `/var/lib/vorpal/key/` directory (TLS keys)
and all data in `/var/lib/vorpal/store/`. Only the binary in `~/.vorpal/bin/` is replaced, and
services are restarted.

#### Phase 5: System Directory Setup

Requires `sudo` for `/var/lib/vorpal/`.

**Interactive sudo prompt:**

```
  Setting up system storage
  [!] Vorpal needs to create /var/lib/vorpal (requires sudo)
```

Then the native `sudo` prompt appears. The installer does NOT ask "Would you like to continue?"
separately -- the sudo prompt IS the confirmation. This eliminates one unnecessary prompt.

**Directories created:**
- `/var/lib/vorpal/key/`
- `/var/lib/vorpal/log/`
- `/var/lib/vorpal/sandbox/`
- `/var/lib/vorpal/store/artifact/{alias,archive,config,output}`

**Ownership:** `chown -R "$(id -u):$(id -g)" /var/lib/vorpal`

**sudo failure** (user cancels or not authorized):

```
  [cross] Could not create system directories (sudo required)

  Vorpal needs /var/lib/vorpal for artifact storage and service logs.
  This directory requires root permissions to create.

  Options:
    [bullet] Re-run and enter your password when prompted
    [bullet] Ask your system administrator for sudo access
    [bullet] Create the directory manually:
        sudo mkdir -p /var/lib/vorpal/{key,log,sandbox,store}
        sudo mkdir -p /var/lib/vorpal/store/artifact/{alias,archive,config,output}
        sudo chown -R $(id -u):$(id -g) /var/lib/vorpal
      Then re-run the installer with: --no-service
```

**On upgrade:** If `/var/lib/vorpal/` already exists with correct ownership, skip this phase
entirely. Print: `[check] System storage (exists)`

#### Phase 6: TLS Key Generation

Invokes `"$HOME/.vorpal/bin/vorpal" system keys generate`.

This command is idempotent -- it checks for existing keys before generating. On upgrade, existing
keys are preserved automatically (the Rust code checks `if !path.exists()` before each key
generation).

```
  Generating security keys
  [check] Generated CA certificate
  [check] Generated service certificate
```

**On failure:**

```
  [cross] Failed to generate security keys

  Error: [stderr output from vorpal system keys generate]

  This is unexpected. Please report this issue:
    https://github.com/ALT-F4-LLC/vorpal/issues

  Include your platform info: macOS aarch64 (Apple Silicon)
```

#### Phase 7: Service Installation

**macOS (LaunchAgent):**

1. Write plist to `~/Library/LaunchAgents/com.altf4llc.vorpal.plist`
2. If plist already loaded: `launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/com.altf4llc.vorpal.plist 2>/dev/null` (ignore errors -- handles fresh install where nothing is loaded)
3. Load: `launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.altf4llc.vorpal.plist`

Note: `launchctl bootstrap/bootout` is the modern replacement for `load/unload`. The current
installer uses the deprecated `load/unload` commands. The redesign uses `bootstrap/bootout`
which are supported on macOS 10.10+ and provide better error reporting.

**Linux (systemd):**

1. Write unit to `/etc/systemd/system/vorpal.service` (requires sudo)
2. `sudo systemctl daemon-reload`
3. `sudo systemctl enable vorpal.service`
4. `sudo systemctl start vorpal.service`

**Verification:** After starting, wait up to 5 seconds with polling to confirm the service is
running:

- macOS: `launchctl print gui/$(id -u)/com.altf4llc.vorpal` should show active state
- Linux: `systemctl is-active vorpal.service` should return `active`

**Service failure:**

```
  [cross] Services failed to start

  The Vorpal background service was installed but did not start successfully.

  Check the logs:
    macOS: cat /var/lib/vorpal/log/services.log
    Linux: journalctl -u vorpal.service --no-pager -n 20

  Common causes:
    [bullet] Port conflict -- another service is using the Vorpal socket
    [bullet] Permission issue -- check /var/lib/vorpal ownership

  Restart manually:
    macOS: launchctl kickstart gui/$(id -u)/com.altf4llc.vorpal
    Linux: sudo systemctl restart vorpal.service
```

**--no-service flag:** Skip this entire phase. Print:

```
  [arrow] Skipping service installation (--no-service)
```

#### Phase 8: Shell PATH Configuration

The installer detects all shells the user has configured and writes PATH additions to the
appropriate rc file for each.

**Detection logic:**

1. Read the user's default shell from `$SHELL`
2. Check for the existence of known rc files regardless of `$SHELL` (user may use multiple shells)

**Shell rc file mapping:**

| Shell | rc file | PATH line |
|---|---|---|
| bash | `~/.bashrc` (Linux), `~/.bash_profile` (macOS) | `export PATH="$HOME/.vorpal/bin:$PATH"` |
| zsh | `~/.zshrc` | `export PATH="$HOME/.vorpal/bin:$PATH"` |
| fish | `~/.config/fish/config.fish` | `fish_add_path $HOME/.vorpal/bin` |

**Guard:** Before writing, check if `$HOME/.vorpal/bin` is already in the file. Use a comment
marker for identification:

```bash
# Vorpal (https://github.com/ALT-F4-LLC/vorpal)
export PATH="$HOME/.vorpal/bin:$PATH"
```

If the marker is found, skip writing. Print: `[check] PATH already configured in ~/.zshrc`

**After writing:**

```
  Configuring shell
  [check] Added ~/.vorpal/bin to PATH in ~/.zshrc
  [!] Open a new terminal or run: source ~/.zshrc
```

The "source" hint is important -- without it, the user will try `vorpal` immediately and get
"command not found," which feels like the install failed.

**--no-path flag:** Skip this phase. Print:

```
  [arrow] Skipping PATH configuration (--no-path)
  [!] Add ~/.vorpal/bin to your PATH manually
```

**No recognized shell:**

```
  [!] Could not detect your shell configuration
  [arrow] Add this to your shell's rc file:
      export PATH="$HOME/.vorpal/bin:$PATH"
```

#### Phase 9: Summary

The summary is the last thing the user sees. It must answer: "Did it work? What do I do now?"

```
  -------------------------------------------------------

  Vorpal nightly installed successfully.

  Get started:
    mkdir hello-world && cd hello-world
    vorpal init hello-world
    vorpal build hello-world

  Docs:     https://github.com/ALT-F4-LLC/vorpal
  Issues:   https://github.com/ALT-F4-LLC/vorpal/issues
```

The divider line is in dim text. "installed successfully" is in bold green. The get-started
commands are copy-paste ready. Links are in cyan (clickable in modern terminals).

### 4.2 Upgrade Flow

Triggered when `~/.vorpal/bin/vorpal` exists and the user runs the installer again (or explicitly
passes `--version`).

**Differences from fresh install:**
- Phase 4: Shows version comparison (`nightly (2026-03-15) -> nightly (2026-03-20)`)
- Phase 5: Skipped if directories exist with correct ownership
- Phase 6: Skipped (keys are idempotent; existing keys are preserved)
- Phase 7: Service is restarted instead of installed
- Phase 8: Skipped if PATH is already configured

**Upgrade summary:**

```
  -------------------------------------------------------

  Vorpal upgraded to nightly (2026-03-20).

  Previous: nightly (2026-03-15)
  Keys:     preserved
  Services: restarted
```

### 4.3 Uninstall Flow

Triggered by `--uninstall` flag.

**Uninstall confirmation** (interactive mode):

```
  This will remove:
    [bullet] Binary:       ~/.vorpal/
    [bullet] System data:  /var/lib/vorpal/
    [bullet] Service:      LaunchAgent / systemd unit
    [bullet] Shell config: PATH entries in shell rc files

  All build artifacts and cached data will be permanently deleted.

  Continue? [y/N]
```

Default is `N` (safe default for destructive actions). In non-interactive mode, uninstall
requires explicit `--yes` flag; without it, abort with a message explaining the requirement.

**Uninstall steps:**

1. Stop service (launchctl bootout / systemctl stop + disable)
2. Remove service configuration file
3. Remove `~/.vorpal/` directory
4. Remove `/var/lib/vorpal/` directory (requires sudo)
5. Remove PATH lines from shell rc files (find and remove the `# Vorpal` block)

**Uninstall summary:**

```
  [check] Vorpal has been uninstalled.

  Removed:
    [bullet] ~/.vorpal/
    [bullet] /var/lib/vorpal/
    [bullet] LaunchAgent configuration
    [bullet] PATH entries in ~/.zshrc
```

---

## 5. Visual & Sensory Design

### The "Magic" Feeling

The magic is not in animation or flashiness. It is in removing friction -- the installer does the
right thing at every step, the user never wonders what is happening, and when it is done, things
just work.

Specific design choices that create this feeling:

1. **Zero questions in the happy path.** The current installer asks "Would you like to continue?"
   before doing anything. The redesigned installer only prompts when it genuinely needs input:
   sudo (unavoidable), and existing installation handling. Everything else has smart defaults.

2. **PATH works immediately.** The biggest friction point in CLI tool installation is "command not
   found" after install. Automatic shell configuration with a clear "source" hint eliminates this.

3. **Rich but not noisy output.** Every step is visible but not verbose. The spinner gives life
   to the experience. Success checkmarks give visual confidence. But there are no unnecessary
   messages, no walls of text, no "verbose" dumps.

4. **Timing information.** Showing elapsed time for download (`4.2 MB, 1.2s`) gives the feeling
   of speed. If it IS fast, the user sees proof. If it is slow, they know it is working.

5. **The summary is a launchpad.** Ending with copy-paste quickstart commands turns "install
   complete" into "ready to build." The user's next action is obvious.

6. **Error messages are conversations, not stack traces.** Every error says what happened, why,
   and what to do. The user is never left staring at a cryptic message.

### Density

The installer output is moderately dense -- tighter than verbose logging, looser than a progress
bar alone. Each step is one line. Details (paths, timing) appear inline. Error details expand
below the failed step. This density works because the install process is short (6-9 steps).

### Motion

The only motion is the spinner on active steps. It provides "alive" feedback without being
distracting. Spinners are 80ms per frame -- fast enough to feel responsive, slow enough not to
flicker.

---

## 6. Edge Cases & Error States

### Empty States

| State | Behavior |
|---|---|
| No internet connection | Download fails with curl error. Message: "Could not reach GitHub. Check your internet connection and try again." |
| GitHub rate limited | HTTP 403 from API. Message: "GitHub API rate limit reached. Wait a few minutes and try again, or download manually from [releases URL]." |
| Disk full | tar extraction or directory creation fails. Message: "Not enough disk space. Vorpal requires approximately 50 MB. Free space and try again." |
| `/var/lib` does not exist | `sudo mkdir` fails. Message includes the specific path and suggests creating parent directories. |

### Overloaded States

Not applicable -- installer is a one-shot process, not a long-running application.

### Degraded States

| Degradation | Behavior |
|---|---|
| No sudo access | Everything except system directories and service setup works. Clear message about what was skipped and how to complete manually. |
| Service fails to start | Installation continues. Binary is available. Clear message about checking logs and restarting manually. |
| Shell rc file is read-only | PATH configuration skipped with a message. Manual instructions provided. |
| Existing installation is corrupted | Binary exists but does not execute. Offer to re-download and replace. |

### Concurrency

| Scenario | Behavior |
|---|---|
| Multiple simultaneous installs | Second instance detects `~/.vorpal/bin/vorpal` is being written (partial file). Use a temp directory for download, then atomic `mv` to final location. |
| Service already running during upgrade | Graceful restart: stop existing, wait for process exit (up to 5s), then start new version. |

### Signal Handling

| Signal | Behavior |
|---|---|
| SIGINT (Ctrl+C) | Clean up any temp files. Print: "Installation cancelled." Exit 130. |
| SIGTERM | Same as SIGINT. |
| SIGHUP | Ignored (continue install if terminal disconnects mid-pipe). |

The installer registers a trap for cleanup:

```
temp_dir=""
cleanup() {
    [ -n "$temp_dir" ] && rm -rf "$temp_dir"
}
trap cleanup EXIT
```

---

## 7. Accessibility

### NO_COLOR Support

When the `NO_COLOR` environment variable is set (any value), or when stdout is not a TTY:

- All ANSI escape codes are suppressed
- Unicode symbols are replaced with ASCII equivalents
- Spinner animation is replaced with a single static line per step
- Banner ASCII art is suppressed (only the text line "Vorpal <version>" is shown)

Implementation: A single function `has_color()` checks these conditions. All output goes through
formatting functions that branch on this flag.

### Screen Reader Friendliness

- Every symbol has a text equivalent (the `[ok]`, `[FAIL]` fallbacks)
- No information is conveyed solely through color
- Progress steps use clear text labels, not just symbols
- The spinner in non-TTY mode produces clean line-by-line output

### Keyboard

Not applicable (non-interactive script). The only interactive elements are:
- `read -p` prompts: accept y/n keyboard input, with timeout and default
- `sudo` prompt: standard system behavior

---

## 8. Internationalization

The installer is English-only for now. Design considerations for future i18n:

- All user-facing strings are defined at the top of the script (or in functions), not inline.
  This is a structural choice that makes future extraction feasible.
- No string concatenation for sentences (avoids word-order assumptions)
- Technical terms (Vorpal, PATH, sudo, LaunchAgent, systemd) are never translated

---

## 9. Privacy & Data Minimization

### Data Inventory

| Data | Collection | Storage | Justification |
|---|---|---|---|
| Platform (arch + OS) | Sent to GitHub as part of download URL | Not stored | Required to download correct binary |
| IP address | GitHub sees it during download | GitHub's policy | Unavoidable for HTTP download |
| Shell type | Detected locally | Not stored | Used only for PATH configuration |
| TLS keys | Generated locally | `/var/lib/vorpal/key/` | Required for gRPC service auth |

### Telemetry

The installer collects no telemetry. No analytics pings, no install-success callbacks, no usage
tracking. If telemetry is added in the future, it must be:
1. Opt-in (not opt-out)
2. Announced during install with a clear explanation
3. Controllable via an environment variable

---

## 10. Measurement

### Key Metrics

| Metric | Source | Target |
|---|---|---|
| Install completion rate | GitHub issue volume tagged "installer" | <5% issue rate |
| Time to install | Timing data in installer output | <60 seconds |
| Upgrade success rate | Issue volume for upgrade-related bugs | <2% issue rate |
| PATH configuration success | Issue volume for "command not found" after install | Near zero |

### Instrumentation Points

The installer itself has no instrumentation (see Privacy section). Measurement comes from:
1. GitHub issue tracking (tag: `installer`)
2. Manual testing across platform matrix before each release
3. CI validation (the `curl | bash` pattern can be tested in CI runners)

### Iteration Triggers

| Signal | Action |
|---|---|
| >3 issues for same error in 30 days | Improve that error message or fix root cause |
| "Command not found" reports after install | Review PATH configuration logic |
| Service start failures on specific platform | Add platform-specific handling |

---

## 11. Handoff Notes

### Component Breakdown

The installer should be implemented as a single `install.sh` file with well-defined internal
functions. No external dependencies beyond `curl` and `tar`.

| Component | Description | Priority |
|---|---|---|
| `main()` | Orchestrates the install flow: parse args, run phases, print summary | MVP |
| `parse_args()` | Argument parsing: -y, -v, --no-service, --no-path, --uninstall, -h | MVP |
| `detect_platform()` | Architecture + OS detection with user-facing labels | MVP |
| `check_prerequisites()` | Verify curl and tar are available | MVP |
| `resolve_version()` | Map version input to download URL; validate version exists | MVP |
| `download_binary()` | Download, extract to temp dir, atomic move to `~/.vorpal/bin/` | MVP |
| `setup_system_dirs()` | Create `/var/lib/vorpal/` tree with sudo | MVP |
| `generate_keys()` | Invoke `vorpal system keys generate` | MVP |
| `install_service_macos()` | Write plist, bootstrap LaunchAgent | MVP |
| `install_service_linux()` | Write systemd unit, enable, start | MVP |
| `verify_service()` | Poll service health for up to 5 seconds | MVP |
| `configure_path()` | Detect shells, write PATH to rc files with guard | MVP |
| `run_uninstall()` | Stop services, remove files, remove PATH entries | MVP |
| `has_color()` | Check NO_COLOR, TTY status | MVP |
| `print_banner()` | ASCII art + version + platform | MVP |
| `print_step()` | Spinner + label + status formatting | MVP |
| `print_error()` | Structured error with what/why/what-to-do | MVP |
| `print_summary()` | Final summary with quickstart commands | MVP |
| `cleanup()` | Trap handler for temp file removal | MVP |

### Technology Recommendations

- **Pure bash** -- no awk, sed, or other tools beyond curl and tar for the core flow. This
  maximizes portability.
- **Minimum bash version:** 3.2 (ships with macOS). Use `[[ ]]` but avoid bash 4+ features like
  associative arrays.
- **Atomic file operations:** Download to a temp directory, verify, then `mv` to the final
  location. Never write directly to `~/.vorpal/bin/`.
- **POSIX signal handling:** `trap cleanup EXIT` for temp file cleanup on any exit path.

### MVP vs. Polish Priorities

**MVP (launch blocker):**
- All phases 1-9 of the fresh install flow
- Non-interactive mode
- Error messages for all failure points
- NO_COLOR support
- Shell PATH configuration (bash, zsh, fish)

**Post-MVP polish:**
- Upgrade flow with version comparison display
- Uninstall flow
- SHA-256 checksum verification (requires checksums to be published alongside release tarballs)
- `--version latest` resolution via GitHub API
- Timing display on download step

### Open Questions

1. **Checksum files:** The current release pipeline does not publish SHA-256 checksums alongside
   tarballs. Should the pipeline be updated to publish `vorpal-{arch}-{os}.tar.gz.sha256` files?
   This is a prerequisite for download verification. The build provenance attestation exists but
   is not easily consumable from a shell script.

2. **`get.vorpal.sh` domain:** The project context mentions `curl -fsSL https://get.vorpal.sh | bash`
   as the target pattern. The current README uses a raw GitHub URL. Is the `get.vorpal.sh`
   domain set up or planned?

3. **Service component selection:** The current installer TODO mentions "support installing only
   specific components (registry, worker, etc.)." Should version 1 of the installer support
   component selection, or should this be deferred?

4. **systemd user services:** The current design uses system-level systemd units
   (`/etc/systemd/system/`), which require sudo. An alternative is user-level systemd units
   (`~/.config/systemd/user/`), which avoid sudo for the service phase. The tradeoff is that user
   services only run when the user is logged in. Which is preferred?

### Dependencies

- **On release pipeline:** The installer depends on the artifact naming convention
  `vorpal-{arch}-{os}.tar.gz` and the GitHub release tag structure. Any changes to the release
  pipeline must be coordinated with installer updates.
- **On `vorpal system keys generate`:** The installer invokes this CLI command. Changes to its
  interface, output, or behavior must be coordinated.
- **On service commands:** The plist and systemd unit reference
  `vorpal system services start`. Changes to this command path must be coordinated.
