#!/bin/bash
set -euo pipefail

# =============================================================================
# Vorpal Installer
# =============================================================================
# Usage: curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash
#
# Environment variables:
#   VORPAL_NONINTERACTIVE=1    Enable non-interactive mode
#   CI=true                    Enable non-interactive mode
#   VORPAL_VERSION=<ver>       Version to install (default: nightly)
#   VORPAL_NO_SERVICE=1        Skip service installation
#   VORPAL_NO_PATH=1           Skip PATH configuration
#   NO_COLOR=1                 Disable color output
# =============================================================================

# -- Constants ----------------------------------------------------------------

VORPAL_VERSION="${VORPAL_VERSION:-nightly}"
VORPAL_INSTALL_DIR="$HOME/.vorpal"
VORPAL_SYSTEM_DIR="/var/lib/vorpal"
VORPAL_REPO="ALT-F4-LLC/vorpal"
VORPAL_GITHUB_URL="https://github.com/$VORPAL_REPO"

# -- Flags (set by parse_args) -----------------------------------------------

FLAG_YES=0
FLAG_UNINSTALL=0
NO_SERVICE="${VORPAL_NO_SERVICE:-0}"
NO_PATH="${VORPAL_NO_PATH:-0}"

# -- State (set during execution) --------------------------------------------

TEMP_DIR=""
ARCH=""
ARCH_LABEL=""
OS=""
OS_LABEL=""
PLATFORM=""
DOWNLOAD_URL=""
RESOLVED_VERSION=""
IS_UPGRADE=0
EXISTING_VERSION=""
SPINNER_PID=""

# -- Output utilities ---------------------------------------------------------

has_color() {
    if [[ -n "${NO_COLOR:-}" ]]; then
        return 1
    fi
    if [[ ! -t 1 ]]; then
        return 1
    fi
    return 0
}

has_unicode() {
    if ! has_color; then
        return 1
    fi
    # Test if terminal renders the checkmark as a single-width character
    local test_width
    test_width="$(printf '\xe2\x9c\x94' 2>/dev/null | wc -m)"
    # Trim whitespace from wc output (macOS wc pads with spaces)
    test_width="$(printf '%s' "$test_width" | tr -d ' ')"
    if [[ "$test_width" = "1" ]]; then
        return 0
    fi
    return 1
}

is_interactive() {
    if [[ "$FLAG_YES" = 1 ]]; then
        return 1
    fi
    if [[ "${VORPAL_NONINTERACTIVE:-0}" = "1" ]]; then
        return 1
    fi
    if [[ "${CI:-}" = "true" ]]; then
        return 1
    fi
    if [[ ! -t 0 ]]; then
        return 1
    fi
    return 0
}

# Formatting helpers — resolve symbols and colors once after detection

_fmt_reset=""
_fmt_bold=""
_fmt_dim=""
_fmt_cyan=""
_fmt_green=""
_fmt_yellow=""
_fmt_red=""

_sym_check=""
_sym_cross=""
_sym_arrow=""
_sym_bullet=""
_sym_warning=""

_setup_formatting() {
    if has_color; then
        _fmt_reset=$'\033[0m'
        _fmt_bold=$'\033[1m'
        _fmt_dim=$'\033[2m'
        _fmt_cyan=$'\033[36;1m'
        _fmt_green=$'\033[32m'
        _fmt_yellow=$'\033[33m'
        _fmt_red=$'\033[31;1m'
    fi

    if has_unicode; then
        _sym_check=$'\xe2\x9c\x94'
        _sym_cross=$'\xe2\x9c\x98'
        _sym_arrow=$'\xe2\x86\x92'
        _sym_bullet=$'\xe2\x80\xa2'
        _sym_warning="!"
    else
        _sym_check="[ok]"
        _sym_cross="[FAIL]"
        _sym_arrow="->"
        _sym_bullet="-"
        _sym_warning="!"
    fi
}

print_banner() {
    # Suppress banner when stdout is not a TTY
    if [[ ! -t 1 ]]; then
        return 0
    fi

    printf '%s' "${_fmt_cyan}"
    cat <<'BANNER'
                           __
 _   _____  _________  ____ _/ /
| | / / __ \/ ___/ __ \/ __ `/ /
| |/ / /_/ / /  / /_/ / /_/ / /
|___/\____/_/  / .___/\__,_/_/
              /_/
BANNER
    printf '%s\n' "${_fmt_reset}"
    printf '  %s\n' "Build system that works as code."
    printf '\n'
    printf '  Version:  %s%s%s\n' "${_fmt_dim}" "$VORPAL_VERSION" "${_fmt_reset}"
    printf '  Platform: %s%s (%s)%s\n' "${_fmt_dim}" "$PLATFORM" "${ARCH}-${OS}" "${_fmt_reset}"
}

print_header() {
    printf '\n  %s%s%s\n' "${_fmt_cyan}${_fmt_bold}" "$1" "${_fmt_reset}"
}

print_step() {
    printf '  %s%s%s %s\n' "${_fmt_bold}" "${_sym_bullet}" "${_fmt_reset}" "$1"
}

print_success() {
    printf '  %s%s%s %s\n' "${_fmt_green}" "${_sym_check}" "${_fmt_reset}" "$1"
}

print_warning() {
    printf '  %s[%s]%s %s\n' "${_fmt_yellow}" "${_sym_warning}" "${_fmt_reset}" "$1"
}

print_error() {
    local what="${1:-}"
    local why="${2:-}"
    local fix="${3:-}"

    printf '\n  %s%s %s%s\n' "${_fmt_red}" "${_sym_cross}" "$what" "${_fmt_reset}"
    if [[ -n "$why" ]]; then
        printf '\n  %s\n' "$why"
    fi
    if [[ -n "$fix" ]]; then
        printf '\n  %s\n' "$fix"
    fi
}

# -- Spinner ------------------------------------------------------------------

spin() {
    local message="${1:-}"

    # No-op when stdout is not a TTY or NO_COLOR is set
    if [[ ! -t 1 ]] || [[ -n "${NO_COLOR:-}" ]]; then
        if [[ -n "$message" ]]; then
            print_step "$message"
        fi
        return 0
    fi

    # Select character set based on Unicode support
    local frames
    if has_unicode; then
        # Braille pattern cycle: U+2801 U+2802 U+2804 U+2840 U+2820 U+2810 U+2808 U+2800
        frames=(
            $'\xe2\xa0\x81'
            $'\xe2\xa0\x82'
            $'\xe2\xa0\x84'
            $'\xe2\xa1\x80'
            $'\xe2\xa0\xa0'
            $'\xe2\xa0\x90'
            $'\xe2\xa0\x88'
            $'\xe2\xa0\x80'
        )
    else
        # ASCII fallback
        frames=("-" "\\" "|" "/")
    fi

    local frame_count=${#frames[@]}

    # Run spinner as a background subshell writing to stderr
    (
        local i=0
        while true; do
            printf '\r  %s%s%s %s' "${_fmt_cyan}" "${frames[$((i % frame_count))]}" "${_fmt_reset}" "$message" >&2
            i=$((i + 1))
            sleep 0.08
        done
    ) &
    SPINNER_PID=$!
}

spin_stop() {
    local status="${1:-success}"
    local message="${2:-}"

    # Kill spinner process if running
    if [[ -n "$SPINNER_PID" ]]; then
        kill "$SPINNER_PID" 2>/dev/null || true
        wait "$SPINNER_PID" 2>/dev/null || true
        SPINNER_PID=""
        # Clear the spinner line
        printf '\r\033[2K' >&2
    fi

    # In non-TTY / NO_COLOR mode, spin() printed a static step line but no
    # result indicator — print the final status so CI logs show the outcome.
    if [[ ! -t 1 ]] || [[ -n "${NO_COLOR:-}" ]]; then
        if [[ "$status" = "success" ]]; then
            print_success "$message"
        else
            print_error "$message"
        fi
        return 0
    fi

    # Print final status
    if [[ "$status" = "success" ]]; then
        print_success "$message"
    else
        print_error "$message"
    fi
}

# -- Platform detection -------------------------------------------------------

detect_arch() {
    local raw_arch
    raw_arch="$(uname -m | tr '[:upper:]' '[:lower:]')"

    case "$raw_arch" in
        x86_64)
            ARCH="x86_64"
            ARCH_LABEL="x86_64"
            ;;
        aarch64|arm64)
            ARCH="aarch64"
            # Label depends on OS — set in detect_platform
            ARCH_LABEL="aarch64"
            ;;
        *)
            ARCH="$raw_arch"
            ARCH_LABEL="$raw_arch"
            ;;
    esac
}

detect_os() {
    local raw_os
    raw_os="$(uname -s)"

    case "$raw_os" in
        Darwin)
            OS="darwin"
            OS_LABEL="macOS"
            ;;
        Linux)
            OS="linux"
            OS_LABEL="Linux"
            ;;
        *)
            OS="$(printf '%s' "$raw_os" | tr '[:upper:]' '[:lower:]')"
            OS_LABEL="$raw_os"
            ;;
    esac
}

detect_platform() {
    detect_arch
    detect_os

    # Set user-facing arch label based on OS (per UX spec)
    if [[ "$ARCH" = "aarch64" ]]; then
        if [[ "$OS" = "darwin" ]]; then
            ARCH_LABEL="Apple Silicon"
        else
            ARCH_LABEL="ARM64"
        fi
    fi

    PLATFORM="${OS_LABEL} ${ARCH_LABEL}"

    # Validate supported combinations
    case "${ARCH}-${OS}" in
        aarch64-darwin|x86_64-darwin|aarch64-linux|x86_64-linux)
            # Supported
            ;;
        *)
            print_error \
                "Unsupported platform: ${OS_LABEL} ${ARCH_LABEL}" \
                "Vorpal supports:
    ${_sym_bullet} macOS (Apple Silicon, Intel)
    ${_sym_bullet} Linux (x86_64, ARM64)" \
                "Building from source: ${VORPAL_GITHUB_URL}#contributing"
            exit 1
            ;;
    esac
}

# -- Argument parsing ---------------------------------------------------------

print_help() {
    cat <<EOF
Usage: install.sh [OPTIONS]

Install Vorpal to ~/.vorpal and configure system services.

Options:
  -y, --yes              Run in non-interactive mode (skip prompts)
  -v, --version <ver>    Version to install (default: nightly)
      --no-service       Skip service installation
      --no-path          Skip PATH configuration
      --uninstall        Uninstall Vorpal
  -h, --help             Show this help message

Environment variables:
  VORPAL_NONINTERACTIVE=1    Enable non-interactive mode
  CI=true                    Enable non-interactive mode
  VORPAL_VERSION=<ver>       Version to install (default: nightly)
  VORPAL_NO_SERVICE=1        Skip service installation
  VORPAL_NO_PATH=1           Skip PATH configuration
  NO_COLOR=1                 Disable color output
EOF
}

parse_args() {
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -y|--yes)
                FLAG_YES=1
                shift
                ;;
            -v|--version)
                if [[ $# -lt 2 ]]; then
                    print_error "Missing value for $1" \
                        "The $1 flag requires a version argument." \
                        "Example: install.sh $1 nightly"
                    exit 1
                fi
                VORPAL_VERSION="$2"
                shift 2
                ;;
            --no-service)
                NO_SERVICE=1
                shift
                ;;
            --no-path)
                NO_PATH=1
                shift
                ;;
            --uninstall)
                FLAG_UNINSTALL=1
                shift
                ;;
            -h|--help)
                print_help
                exit 0
                ;;
            *)
                print_error "Unknown option: $1" \
                    "" \
                    "Run 'install.sh --help' for usage information."
                exit 1
                ;;
        esac
    done
}

# -- Signal handling & cleanup ------------------------------------------------

cleanup() {
    # Kill any active spinner to prevent orphan processes
    if [[ -n "$SPINNER_PID" ]]; then
        kill "$SPINNER_PID" 2>/dev/null || true
        wait "$SPINNER_PID" 2>/dev/null || true
        SPINNER_PID=""
    fi
    if [[ -n "$TEMP_DIR" ]] && [[ -d "$TEMP_DIR" ]]; then
        rm -rf "$TEMP_DIR"
    fi
}

handle_signal() {
    printf '\n  Installation cancelled.\n' >&2
    cleanup
    exit 130
}

setup_trap() {
    trap cleanup EXIT
    trap handle_signal INT TERM
    trap '' HUP
}

# -- Phase 1: Prerequisites ---------------------------------------------------

check_prerequisites() {
    print_header "Checking prerequisites"

    local missing=()

    if command -v curl >/dev/null 2>&1; then
        local curl_version
        curl_version="$(curl --version 2>/dev/null | head -1 | awk '{print $2}')"
        print_success "curl ${curl_version}"
    else
        missing+=("curl")
    fi

    if command -v tar >/dev/null 2>&1; then
        local tar_version
        tar_version="$(tar --version 2>&1 | head -1)"
        if [[ -n "$tar_version" ]]; then
            print_success "tar (${tar_version})"
        else
            print_success "tar"
        fi
    else
        missing+=("tar")
    fi

    if [[ ${#missing[@]} -gt 0 ]]; then
        local list=""
        local tool
        for tool in "${missing[@]}"; do
            list="${list}
    ${_sym_bullet} ${tool} -- install via your package manager"
        done
        print_error \
            "Missing required tools:${list}" \
            "Install them and re-run the installer."
        exit 1
    fi
}

# -- Phase 2: Download & version resolution -----------------------------------

resolve_version() {
    local version="$VORPAL_VERSION"
    local artifact="vorpal-${ARCH}-${OS}.tar.gz"

    if [[ "$version" = "latest" ]]; then
        # Query GitHub API for latest non-prerelease tag
        local api_url="https://api.github.com/repos/${VORPAL_REPO}/releases/latest"
        local api_response
        local http_code

        http_code="$(curl -sS -o /dev/null -w "%{http_code}" "$api_url" 2>/dev/null)" || true

        if [[ "$http_code" = "403" ]]; then
            print_warning "GitHub API rate limit reached. Falling back to nightly."
            version="nightly"
        elif [[ "$http_code" = "200" ]]; then
            api_response="$(curl -fsSL "$api_url" 2>/dev/null)" || true
            if [[ -n "$api_response" ]]; then
                # Extract tag_name — simple grep to avoid jq dependency
                local tag
                tag="$(printf '%s' "$api_response" | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"//;s/"//')"
                if [[ -n "$tag" ]]; then
                    version="$tag"
                else
                    print_warning "Could not parse latest version from GitHub API. Falling back to nightly."
                    version="nightly"
                fi
            else
                print_warning "Could not fetch latest version from GitHub API. Falling back to nightly."
                version="nightly"
            fi
        else
            print_warning "GitHub API returned HTTP ${http_code}. Falling back to nightly."
            version="nightly"
        fi
    fi

    RESOLVED_VERSION="$version"
    DOWNLOAD_URL="${VORPAL_GITHUB_URL}/releases/download/${version}/${artifact}"

    # Validate version exists via HTTP HEAD
    local head_code
    head_code="$(curl -fsSI -o /dev/null -w "%{http_code}" "$DOWNLOAD_URL" 2>/dev/null)" || true

    if [[ "$head_code" != "200" ]] && [[ "$head_code" != "302" ]]; then
        print_error \
            "Version \"${VORPAL_VERSION}\" not found." \
            "Available channels:
    ${_sym_bullet} nightly -- latest development build (updated daily)
    ${_sym_bullet} latest  -- most recent stable release" \
            "Or specify an exact tag: --version v0.1.0-alpha.0
  See all releases: ${VORPAL_GITHUB_URL}/releases"
        exit 1
    fi
}

handle_existing() {
    local binary="${VORPAL_INSTALL_DIR}/bin/vorpal"

    if [[ ! -x "$binary" ]]; then
        return 0
    fi

    # Get current installed version
    EXISTING_VERSION="$("$binary" --version 2>/dev/null || printf 'unknown')"

    if ! is_interactive; then
        # Non-interactive: auto-upgrade
        IS_UPGRADE=1
        print_warning "Vorpal is already installed (${EXISTING_VERSION}). Upgrading to ${RESOLVED_VERSION}."
        return 0
    fi

    print_warning "Vorpal is already installed (${EXISTING_VERSION})"
    printf '\n'
    printf '  Options:\n'
    printf '    1) Upgrade to %s [default]\n' "$RESOLVED_VERSION"
    printf '    2) Reinstall %s\n' "$RESOLVED_VERSION"
    printf '    3) Cancel\n'
    printf '\n'
    printf '  Choice [1]: '

    local choice
    read -r choice </dev/tty || choice="1"
    choice="${choice:-1}"

    case "$choice" in
        1)
            IS_UPGRADE=1
            ;;
        2)
            IS_UPGRADE=0
            ;;
        3)
            printf '  Installation cancelled.\n'
            exit 0
            ;;
        *)
            printf '  Installation cancelled.\n'
            exit 0
            ;;
    esac
}

download_binary() {
    print_header "Downloading"

    # Create temp dir for atomic download
    TEMP_DIR="$(mktemp -d)"

    local artifact="vorpal-${ARCH}-${OS}.tar.gz"
    local temp_tarball="${TEMP_DIR}/${artifact}"
    local temp_binary="${TEMP_DIR}/vorpal"

    # Download tarball
    spin "Downloading Vorpal ${RESOLVED_VERSION}..."

    local download_start
    download_start="$(date +%s)"

    if ! curl -fSL -o "$temp_tarball" "$DOWNLOAD_URL" 2>/dev/null; then
        spin_stop "failure"
        print_error \
            "Download failed" \
            "Could not download Vorpal ${RESOLVED_VERSION} for ${ARCH}-${OS}.
  URL: ${DOWNLOAD_URL}" \
            "This usually means:
    ${_sym_bullet} The version does not exist -- check: ${VORPAL_GITHUB_URL}/releases
    ${_sym_bullet} Your platform is not supported for this version
    ${_sym_bullet} GitHub is experiencing an outage -- check: https://www.githubstatus.com"
        exit 1
    fi

    local download_end
    download_end="$(date +%s)"
    local elapsed=$((download_end - download_start))

    # Get file size for display
    local file_size=""
    if command -v stat >/dev/null 2>&1; then
        if [[ "$OS" = "darwin" ]]; then
            file_size="$(stat -f%z "$temp_tarball" 2>/dev/null || printf '')"
        else
            file_size="$(stat -c%s "$temp_tarball" 2>/dev/null || printf '')"
        fi
        if [[ -n "$file_size" ]]; then
            # Convert to MB
            local size_mb
            size_mb="$(awk "BEGIN {printf \"%.1f\", ${file_size}/1048576}")"
            file_size="${size_mb} MB, "
        fi
    fi

    # Extract tarball
    if ! tar xz -C "$TEMP_DIR" -f "$temp_tarball" 2>/dev/null; then
        spin_stop "failure"
        print_error \
            "Extraction failed" \
            "Could not extract the downloaded archive." \
            "Try again: curl -fsSL https://raw.githubusercontent.com/${VORPAL_REPO}/main/script/install.sh | bash
  Report:    ${VORPAL_GITHUB_URL}/issues"
        exit 1
    fi

    # Verify binary exists and is executable
    if [[ ! -f "$temp_binary" ]]; then
        spin_stop "failure"
        print_error \
            "Downloaded binary failed verification" \
            "The archive was extracted but does not contain the expected binary." \
            "Try again: curl -fsSL https://raw.githubusercontent.com/${VORPAL_REPO}/main/script/install.sh | bash
  Report:    ${VORPAL_GITHUB_URL}/issues"
        exit 1
    fi

    chmod +x "$temp_binary"

    # Verify binary via --version
    if ! "$temp_binary" --version >/dev/null 2>&1; then
        spin_stop "failure"
        print_error \
            "Downloaded binary failed verification" \
            "The file was downloaded but does not appear to be a valid Vorpal binary.
  This may indicate a corrupted download or incompatible binary." \
            "Try again: curl -fsSL https://raw.githubusercontent.com/${VORPAL_REPO}/main/script/install.sh | bash
  Report:    ${VORPAL_GITHUB_URL}/issues"
        exit 1
    fi

    spin_stop "success" "Downloaded Vorpal ${RESOLVED_VERSION} (${file_size}${elapsed}s)"

    # Atomic move to final location
    mkdir -p "${VORPAL_INSTALL_DIR}/bin"
    mv -f "$temp_binary" "${VORPAL_INSTALL_DIR}/bin/vorpal"

    print_success "Verified binary (vorpal --version)"

    # Clean up temp dir now that we're done with it
    rm -rf "$TEMP_DIR"
    TEMP_DIR=""
}

# -- Phase stubs (implemented in subsequent phases) ---------------------------

setup_system_dirs() {
    print_header "Setting up system storage"

    local system_dir="$VORPAL_SYSTEM_DIR"
    local current_uid
    local current_gid
    current_uid="$(id -u)"
    current_gid="$(id -g)"

    # On upgrade: skip if directory exists with correct ownership
    if [[ -d "$system_dir" ]]; then
        local dir_owner
        if [[ "$OS" = "darwin" ]]; then
            dir_owner="$(stat -f%u "$system_dir" 2>/dev/null || printf '')"
        else
            dir_owner="$(stat -c%u "$system_dir" 2>/dev/null || printf '')"
        fi

        if [[ "$dir_owner" = "$current_uid" ]]; then
            print_success "System storage (exists)"
            return 0
        fi
    fi

    # Pre-announce sudo requirement per UX spec
    print_warning "Vorpal needs to create ${system_dir} (requires sudo)"

    # Create directories with sudo
    if ! sudo mkdir -p \
        "${system_dir}/key" \
        "${system_dir}/log" \
        "${system_dir}/sandbox" \
        "${system_dir}/store" \
        "${system_dir}/store/artifact/alias" \
        "${system_dir}/store/artifact/archive" \
        "${system_dir}/store/artifact/config" \
        "${system_dir}/store/artifact/output" 2>/dev/null; then
        print_error \
            "Could not create system directories (sudo required)" \
            "Vorpal needs ${system_dir} for artifact storage and service logs.
  This directory requires root permissions to create." \
            "Options:
    ${_sym_bullet} Re-run and enter your password when prompted
    ${_sym_bullet} Ask your system administrator for sudo access
    ${_sym_bullet} Create the directory manually:
        sudo mkdir -p ${system_dir}/{key,log,sandbox,store}
        sudo mkdir -p ${system_dir}/store/artifact/{alias,archive,config,output}
        sudo chown -R \$(id -u):\$(id -g) ${system_dir}
      Then re-run the installer with: --no-service"
        exit 1
    fi

    # Set ownership to current user
    if ! sudo chown -R "${current_uid}:${current_gid}" "$system_dir" 2>/dev/null; then
        print_error \
            "Could not set ownership on system directories (sudo required)" \
            "The directories were created but ownership could not be set." \
            "Run manually:
        sudo chown -R ${current_uid}:${current_gid} ${system_dir}
      Then re-run the installer."
        exit 1
    fi

    print_success "Created system directories"
}

generate_keys() {
    print_header "Generating security keys"

    local vorpal_bin="${VORPAL_INSTALL_DIR}/bin/vorpal"
    local key_output

    spin "Generating security keys..."

    if ! key_output="$("$vorpal_bin" system keys generate 2>&1)"; then
        spin_stop "failure"
        print_error \
            "Failed to generate security keys" \
            "Error: ${key_output}" \
            "This is unexpected. Please report this issue:
    ${VORPAL_GITHUB_URL}/issues

  Include your platform info: ${OS_LABEL} ${ARCH} (${ARCH_LABEL})"
        exit 1
    fi

    spin_stop "success" "Generated security keys"
}

install_service_macos() {
    local plist_dir="${HOME}/Library/LaunchAgents"
    local plist_path="${plist_dir}/com.altf4llc.vorpal.plist"
    local vorpal_bin="${VORPAL_INSTALL_DIR}/bin/vorpal"
    local gui_target="gui/$(id -u)"

    mkdir -p "$plist_dir"

    # Write the plist
    cat > "$plist_path" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.altf4llc.vorpal</string>
    <key>ProgramArguments</key>
    <array>
        <string>${vorpal_bin}</string>
        <string>system</string>
        <string>services</string>
        <string>start</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>${VORPAL_SYSTEM_DIR}/log/services.log</string>
    <key>StandardErrorPath</key>
    <string>${VORPAL_SYSTEM_DIR}/log/services.log</string>
</dict>
</plist>
PLIST

    # Bootout existing service (ignore errors — may not be loaded on fresh install)
    launchctl bootout "${gui_target}/com.altf4llc.vorpal" 2>/dev/null || true

    spin "Starting LaunchAgent..."

    # Bootstrap the service
    if ! launchctl bootstrap "$gui_target" "$plist_path" 2>/dev/null; then
        spin_stop "failure"
        print_error \
            "Services failed to start" \
            "The Vorpal background service was installed but did not start successfully." \
            "Check the logs:
    cat ${VORPAL_SYSTEM_DIR}/log/services.log

  Common causes:
    ${_sym_bullet} Port conflict -- another service is using the Vorpal socket
    ${_sym_bullet} Permission issue -- check ${VORPAL_SYSTEM_DIR} ownership

  Restart manually:
    launchctl kickstart ${gui_target}/com.altf4llc.vorpal"
        return 1
    fi

    spin_stop "success" "Installed LaunchAgent"
}

install_service_linux() {
    local unit_dir="${HOME}/.config/systemd/user"
    local unit_path="${unit_dir}/vorpal.service"
    local vorpal_bin="${VORPAL_INSTALL_DIR}/bin/vorpal"

    mkdir -p "$unit_dir"

    # Write the systemd user unit
    cat > "$unit_path" <<UNIT
[Unit]
Description=Vorpal Build System Services
After=network.target

[Service]
Type=simple
ExecStart=${vorpal_bin} system services start
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
UNIT

    # Stop existing service if running (graceful restart on upgrade)
    systemctl --user stop vorpal.service 2>/dev/null || true

    spin "Starting systemd user service..."

    # Reload, enable, start
    if ! systemctl --user daemon-reload 2>/dev/null; then
        spin_stop "failure"
        print_error \
            "Services failed to start" \
            "Could not reload systemd user daemon." \
            "Check the logs:
    journalctl --user -u vorpal.service --no-pager -n 20

  Restart manually:
    systemctl --user daemon-reload
    systemctl --user restart vorpal.service"
        return 1
    fi

    if ! systemctl --user enable vorpal.service 2>/dev/null; then
        print_warning "Could not enable vorpal.service for auto-start"
    fi

    if ! systemctl --user start vorpal.service 2>/dev/null; then
        spin_stop "failure"
        print_error \
            "Services failed to start" \
            "The Vorpal background service was installed but did not start successfully." \
            "Check the logs:
    journalctl --user -u vorpal.service --no-pager -n 20

  Common causes:
    ${_sym_bullet} Port conflict -- another service is using the Vorpal socket
    ${_sym_bullet} Permission issue -- check ${VORPAL_SYSTEM_DIR} ownership

  Restart manually:
    systemctl --user restart vorpal.service"
        return 1
    fi

    spin_stop "success" "Installed systemd user service"
}

install_service() {
    print_header "Starting services"

    case "$OS" in
        darwin)
            install_service_macos
            ;;
        linux)
            install_service_linux
            ;;
    esac
}

verify_service() {
    local max_wait=5
    local waited=0

    spin "Verifying services..."

    while [[ "$waited" -lt "$max_wait" ]]; do
        case "$OS" in
            darwin)
                if launchctl print "gui/$(id -u)/com.altf4llc.vorpal" 2>/dev/null | grep -q "state = running"; then
                    spin_stop "success" "Services running"
                    return 0
                fi
                ;;
            linux)
                if [[ "$(systemctl --user is-active vorpal.service 2>/dev/null)" = "active" ]]; then
                    spin_stop "success" "Services running"
                    return 0
                fi
                ;;
        esac
        sleep 1
        waited=$((waited + 1))
    done

    spin_stop "failure"

    # Verification failed — warn but do not exit (binary is still installed)
    local log_cmd
    local restart_cmd
    if [[ "$OS" = "darwin" ]]; then
        log_cmd="cat ${VORPAL_SYSTEM_DIR}/log/services.log"
        restart_cmd="launchctl kickstart gui/$(id -u)/com.altf4llc.vorpal"
    else
        log_cmd="journalctl --user -u vorpal.service --no-pager -n 20"
        restart_cmd="systemctl --user restart vorpal.service"
    fi

    print_warning "Services did not become active within ${max_wait}s"
    printf '\n'
    printf '  Check the logs:\n'
    printf '    %s\n' "$log_cmd"
    printf '\n'
    printf '  Restart manually:\n'
    printf '    %s\n' "$restart_cmd"
}

configure_path() {
    print_header "Configuring shell"

    local marker="# Vorpal (https://github.com/ALT-F4-LLC/vorpal)"
    local path_line='export PATH="$HOME/.vorpal/bin:$PATH"'
    local fish_path_line='fish_add_path $HOME/.vorpal/bin'
    local configured=0
    local active_shell_rc=""

    # Determine the user's active shell name from $SHELL
    local active_shell_name=""
    if [[ -n "${SHELL:-}" ]]; then
        active_shell_name="$(basename "$SHELL")"
    fi

    # --- bash ---
    local bash_rc=""
    if [[ "$OS" = "darwin" ]]; then
        bash_rc="$HOME/.bash_profile"
    else
        bash_rc="$HOME/.bashrc"
    fi

    if [[ -f "$bash_rc" ]]; then
        if grep -qF "$marker" "$bash_rc" 2>/dev/null; then
            print_success "PATH already configured in ${bash_rc/#$HOME/~}"
        else
            printf '\n%s\n%s\n' "$marker" "$path_line" >> "$bash_rc"
            print_success "Added ~/.vorpal/bin to PATH in ${bash_rc/#$HOME/~}"
        fi
        configured=1
        if [[ "$active_shell_name" = "bash" ]]; then
            active_shell_rc="$bash_rc"
        fi
    fi

    # --- zsh ---
    local zsh_rc="$HOME/.zshrc"

    if [[ -f "$zsh_rc" ]]; then
        if grep -qF "$marker" "$zsh_rc" 2>/dev/null; then
            print_success "PATH already configured in ~/.zshrc"
        else
            printf '\n%s\n%s\n' "$marker" "$path_line" >> "$zsh_rc"
            print_success "Added ~/.vorpal/bin to PATH in ~/.zshrc"
        fi
        configured=1
        if [[ "$active_shell_name" = "zsh" ]]; then
            active_shell_rc="$zsh_rc"
        fi
    fi

    # --- fish ---
    local fish_rc="$HOME/.config/fish/config.fish"

    if [[ -f "$fish_rc" ]]; then
        if grep -qF "$marker" "$fish_rc" 2>/dev/null; then
            print_success "PATH already configured in ~/.config/fish/config.fish"
        else
            printf '\n%s\n%s\n' "$marker" "$fish_path_line" >> "$fish_rc"
            print_success "Added ~/.vorpal/bin to PATH in ~/.config/fish/config.fish"
        fi
        configured=1
        if [[ "$active_shell_name" = "fish" ]]; then
            active_shell_rc="$fish_rc"
        fi
    fi

    # No recognized shell rc files found
    if [[ "$configured" = 0 ]]; then
        print_warning "Could not detect your shell configuration"
        printf '  %s Add this to your shell'\''s rc file:\n' "${_sym_arrow}"
        printf '      export PATH="$HOME/.vorpal/bin:$PATH"\n'
        return 0
    fi

    # Source hint for the active shell
    if [[ -n "$active_shell_rc" ]]; then
        local display_rc="${active_shell_rc/#$HOME/~}"
        print_warning "Open a new terminal or run: source ${display_rc}"
    fi
}

print_summary() {
    printf '\n'
    printf '  %s-------------------------------------------------------%s\n' "${_fmt_dim}" "${_fmt_reset}"

    if [[ "$IS_UPGRADE" = 1 ]]; then
        printf '\n  %s%sVorpal upgraded to %s.%s\n' "${_fmt_bold}" "${_fmt_green}" "$RESOLVED_VERSION" "${_fmt_reset}"
        printf '\n'
        printf '  Previous: %s\n' "$EXISTING_VERSION"
        printf '  Keys:     preserved\n'
        printf '  Services: restarted\n'
    else
        printf '\n  %s%sVorpal %s installed successfully.%s\n' "${_fmt_bold}" "${_fmt_green}" "$RESOLVED_VERSION" "${_fmt_reset}"
        printf '\n'
        printf '  Get started:\n'
        printf '    mkdir hello-world && cd hello-world\n'
        printf '    vorpal init hello-world\n'
        printf '    vorpal build hello-world\n'
    fi

    printf '\n'
    printf '  Docs:     %s%s%s\n' "${_fmt_cyan}" "${VORPAL_GITHUB_URL}" "${_fmt_reset}"
    printf '  Issues:   %s%s/issues%s\n' "${_fmt_cyan}" "${VORPAL_GITHUB_URL}" "${_fmt_reset}"

    # loginctl enable-linger note for Linux (per TDD section 8, risk #5)
    if [[ "$OS" = "linux" ]]; then
        printf '\n'
        print_warning "To keep services running after logout: loginctl enable-linger"
    fi
}

run_uninstall() {
    local removed=()

    # Confirmation prompt
    if is_interactive; then
        printf '\n  This will remove:\n'
        printf '    %s Binary:       ~/.vorpal/\n' "${_sym_bullet}"
        printf '    %s System data:  /var/lib/vorpal/\n' "${_sym_bullet}"
        if [[ "$OS" = "darwin" ]]; then
            printf '    %s Service:      LaunchAgent\n' "${_sym_bullet}"
        else
            printf '    %s Service:      systemd unit\n' "${_sym_bullet}"
        fi
        printf '    %s Shell config: PATH entries in shell rc files\n' "${_sym_bullet}"
        printf '\n  All build artifacts and cached data will be permanently deleted.\n'
        printf '\n  Continue? [y/N] '

        local confirm
        read -r confirm </dev/tty || confirm=""
        case "$confirm" in
            y|Y|yes|YES)
                ;;
            *)
                printf '  Uninstall cancelled.\n'
                exit 0
                ;;
        esac
    else
        # Non-interactive: require explicit --yes
        if [[ "$FLAG_YES" != 1 ]]; then
            print_error \
                "Uninstall requires confirmation" \
                "Non-interactive uninstall requires the --yes flag." \
                "Run: install.sh --uninstall --yes"
            exit 1
        fi
    fi

    # 1. Stop and remove service
    if [[ "$OS" = "darwin" ]]; then
        local gui_target="gui/$(id -u)"
        launchctl bootout "${gui_target}/com.altf4llc.vorpal" 2>/dev/null || true
        local plist_path="${HOME}/Library/LaunchAgents/com.altf4llc.vorpal.plist"
        if [[ -f "$plist_path" ]]; then
            rm -f "$plist_path"
            removed+=("LaunchAgent configuration")
        fi
    else
        systemctl --user stop vorpal.service 2>/dev/null || true
        systemctl --user disable vorpal.service 2>/dev/null || true
        local unit_path="${HOME}/.config/systemd/user/vorpal.service"
        if [[ -f "$unit_path" ]]; then
            rm -f "$unit_path"
            systemctl --user daemon-reload 2>/dev/null || true
            removed+=("systemd user service")
        fi
    fi

    # 2. Remove ~/.vorpal/
    if [[ -d "$VORPAL_INSTALL_DIR" ]]; then
        rm -rf "$VORPAL_INSTALL_DIR"
        removed+=("${VORPAL_INSTALL_DIR/#$HOME/~}/")
    fi

    # 3. Remove /var/lib/vorpal/ (requires sudo)
    if [[ -d "$VORPAL_SYSTEM_DIR" ]]; then
        if sudo rm -rf "$VORPAL_SYSTEM_DIR" 2>/dev/null; then
            removed+=("$VORPAL_SYSTEM_DIR/")
        else
            print_warning "Could not remove ${VORPAL_SYSTEM_DIR} (sudo required). Remove manually: sudo rm -rf ${VORPAL_SYSTEM_DIR}"
        fi
    fi

    # 4. Remove PATH entries from shell rc files
    local marker="# Vorpal (https://github.com/ALT-F4-LLC/vorpal)"
    local rc_files=()

    if [[ "$OS" = "darwin" ]]; then
        rc_files+=("$HOME/.bash_profile")
    else
        rc_files+=("$HOME/.bashrc")
    fi
    rc_files+=("$HOME/.zshrc")
    rc_files+=("$HOME/.config/fish/config.fish")

    local rc_file
    for rc_file in "${rc_files[@]}"; do
        if [[ -f "$rc_file" ]] && grep -qF "$marker" "$rc_file" 2>/dev/null; then
            # Remove the marker line and the line following it (the PATH/fish_add_path line)
            # Use a temp file to avoid sed -i portability issues between macOS and Linux
            local tmp_file
            tmp_file="$(mktemp)"
            local skip_next=0
            while IFS= read -r line || [[ -n "$line" ]]; do
                if [[ "$skip_next" = 1 ]]; then
                    skip_next=0
                    continue
                fi
                if [[ "$line" = "$marker" ]]; then
                    skip_next=1
                    continue
                fi
                printf '%s\n' "$line"
            done < "$rc_file" > "$tmp_file"
            mv -f "$tmp_file" "$rc_file"
            removed+=("PATH entries in ${rc_file/#$HOME/~}")
        fi
    done

    # Print uninstall summary
    printf '\n'
    print_success "Vorpal has been uninstalled."

    if [[ ${#removed[@]} -gt 0 ]]; then
        printf '\n  Removed:\n'
        local item
        for item in "${removed[@]}"; do
            printf '    %s %s\n' "${_sym_bullet}" "$item"
        done
    fi
}

# -- Orchestration ------------------------------------------------------------

main() {
    parse_args "$@"
    setup_trap
    _setup_formatting
    detect_platform

    if [[ "$FLAG_UNINSTALL" = 1 ]]; then
        run_uninstall
        exit 0
    fi

    print_banner
    check_prerequisites
    resolve_version
    handle_existing
    download_binary
    setup_system_dirs
    generate_keys

    if [[ "$NO_SERVICE" != 1 ]]; then
        install_service
        verify_service
    else
        printf '\n  %s Skipping service installation (--no-service)\n' "${_sym_arrow}"
    fi

    if [[ "$NO_PATH" != 1 ]]; then
        configure_path
    else
        printf '\n  %s Skipping PATH configuration (--no-path)\n' "${_sym_arrow}"
        print_warning "Add ~/.vorpal/bin to your PATH manually"
    fi

    print_summary
}

main "$@"
