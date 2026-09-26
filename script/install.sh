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
#   VORPAL_VERSION=<ver>       Version to install (default: 0.4.1)
#   VORPAL_NO_SERVICE=1        Skip service installation
#   VORPAL_SERVICES=<list>     Comma-separated services to install (default: agent,registry,worker)
#   VORPAL_NO_PATH=1           Skip PATH configuration
#   VORPAL_DRY_RUN=1           Show what would be done without making changes
#   VORPAL_ISSUER=<url>        OIDC issuer URL for authenticated services
#   VORPAL_ISSUER_AUDIENCE=<v> OIDC issuer audience
#   VORPAL_ISSUER_CLIENT_ID=<v>    OIDC issuer client ID
#   VORPAL_ISSUER_CLIENT_SECRET=<v> OIDC issuer client secret
#   NO_COLOR=1                 Disable color output
#
# Tested from Rust: the `installer_*` tests in cli/src/command.rs run this
# file — as a program for the refusal paths, and sourced for its individual
# validators and writers — so renaming a function or changing a refusal
# message here breaks `cargo test -p vorpal-cli`.
# =============================================================================

# -- Constants ----------------------------------------------------------------

VORPAL_VERSION="${VORPAL_VERSION:-0.4.1}"
VORPAL_INSTALL_DIR="$HOME/.vorpal"
VORPAL_SYSTEM_DIR="/var/lib/vorpal"
VORPAL_REPO="ALT-F4-LLC/vorpal"
VORPAL_GITHUB_URL="https://github.com/$VORPAL_REPO"

# -- Flags (set by parse_args) -----------------------------------------------

FLAG_YES=0
FLAG_UNINSTALL=0
FLAG_DRY_RUN="${VORPAL_DRY_RUN:-0}"
NO_SERVICE="${VORPAL_NO_SERVICE:-0}"
NO_PATH="${VORPAL_NO_PATH:-0}"
SERVICES="${VORPAL_SERVICES:-agent,registry,worker}"
ISSUER="${VORPAL_ISSUER:-}"
ISSUER_AUDIENCE="${VORPAL_ISSUER_AUDIENCE:-}"
ISSUER_CLIENT_ID="${VORPAL_ISSUER_CLIENT_ID:-}"
ISSUER_CLIENT_SECRET="${VORPAL_ISSUER_CLIENT_SECRET:-}"

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

# The temporary sibling a create-then-rename writer is part way through, or
# "" when no write is in flight. The env file and the plist carry the OIDC
# client secret, so an interrupt between "create" and "rename" would otherwise
# leave that secret at a second path nothing ever removes; `cleanup` unlinks
# whatever this names.
PENDING_WRITE_TMP=""

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
 _   _ ____  _________  ____ _/ /
| | / / __ \/ ___/ __ \/ __ `/ /
| |/ / /_/ / /  / /_/ / /_/ / /
|___/\____/_/  / .___/\__,_/_/
              /_/
BANNER
    printf '%s' "${_fmt_reset}"
    printf '\n  %s\n' "Build system that works as code."
    printf '\n'
    printf '  Version:  %s%s%s\n' "${_fmt_dim}" "$VORPAL_VERSION" "${_fmt_reset}"
    printf '  Platform: %s%s (%s)%s\n' "${_fmt_dim}" "$PLATFORM" "${ARCH}-${OS}" "${_fmt_reset}"
    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        printf '\n  %s%s[DRY RUN]%s %sNo changes will be made%s\n' \
            "${_fmt_bold}" "${_fmt_yellow}" "${_fmt_reset}" \
            "${_fmt_dim}" "${_fmt_reset}"
    fi
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

    # Run spinner as a background subshell writing to stderr. Elapsed time is
    # derived from the loop's own iteration count (~0.08s/tick) rather than
    # shelling out to `date` on every frame, so no extra process/IPC is
    # needed to carry live status into the subshell.
    (
        local i=0
        local elapsed=0
        local elapsed_label=""
        while true; do
            elapsed=$(( (i * 8) / 100 ))
            if [[ "$elapsed" -ge 2 ]]; then
                elapsed_label=" (${elapsed}s)"
            fi
            printf '\r  %s%s%s %s%s' "${_fmt_cyan}" "${frames[$((i % frame_count))]}" "${_fmt_reset}" "$message" "$elapsed_label" >&2
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
        if [[ -n "$message" ]]; then
            if [[ "$status" = "success" ]]; then
                print_success "$message"
            else
                print_error "$message"
            fi
        fi
        return 0
    fi

    # Print final status (empty message = stop and clear only, no line printed)
    if [[ -n "$message" ]]; then
        if [[ "$status" = "success" ]]; then
            print_success "$message"
        else
            print_error "$message"
        fi
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
  -v, --version <ver>    Version to install (default: 0.4.1)
      --services <list>  Comma-separated services to install (default: agent,registry,worker)
      --no-service       Skip service installation
      --no-path          Skip PATH configuration
      --dry-run          Show what would be done without making changes
      --uninstall        Uninstall Vorpal
      --issuer <url>             OIDC issuer URL
      --issuer-audience <value>  OIDC issuer audience
      --issuer-client-id <value> OIDC issuer client ID
      --issuer-client-secret <value> OIDC issuer client secret
  -h, --help             Show this help message

Environment variables:
  VORPAL_NONINTERACTIVE=1    Enable non-interactive mode
  CI=true                    Enable non-interactive mode
  VORPAL_VERSION=<ver>       Version to install (default: 0.4.1)
  VORPAL_NO_SERVICE=1        Skip service installation
  VORPAL_SERVICES=<list>     Comma-separated services (default: agent,registry,worker)
  VORPAL_NO_PATH=1           Skip PATH configuration
  VORPAL_DRY_RUN=1           Show what would be done without making changes
  VORPAL_ISSUER=<url>        OIDC issuer URL
  VORPAL_ISSUER_AUDIENCE=<v> OIDC issuer audience
  VORPAL_ISSUER_CLIENT_ID=<v>    OIDC issuer client ID
  VORPAL_ISSUER_CLIENT_SECRET=<v> OIDC issuer client secret
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
                        "Example: install.sh $1 0.4.1"
                    exit 1
                fi
                VORPAL_VERSION="$2"
                shift 2
                ;;
            --services)
                if [[ $# -lt 2 ]]; then
                    print_error "Missing value for $1" \
                        "The $1 flag requires a comma-separated list of services." \
                        "Example: install.sh $1 agent,registry,worker"
                    exit 1
                fi
                SERVICES="$2"
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
            --dry-run)
                FLAG_DRY_RUN=1
                shift
                ;;
            --issuer)
                if [[ $# -lt 2 ]]; then
                    print_error "Missing value for $1" \
                        "The $1 flag requires a URL argument." \
                        "Example: install.sh $1 https://accounts.example.com"
                    exit 1
                fi
                ISSUER="$2"
                shift 2
                ;;
            --issuer-audience)
                if [[ $# -lt 2 ]]; then
                    print_error "Missing value for $1" \
                        "The $1 flag requires a value argument." \
                        "Example: install.sh $1 my-audience"
                    exit 1
                fi
                ISSUER_AUDIENCE="$2"
                shift 2
                ;;
            --issuer-client-id)
                if [[ $# -lt 2 ]]; then
                    print_error "Missing value for $1" \
                        "The $1 flag requires a value argument." \
                        "Example: install.sh $1 my-client-id"
                    exit 1
                fi
                ISSUER_CLIENT_ID="$2"
                shift 2
                ;;
            --issuer-client-secret)
                if [[ $# -lt 2 ]]; then
                    print_error "Missing value for $1" \
                        "The $1 flag requires a value argument." \
                        "Example: install.sh $1 my-client-secret"
                    exit 1
                fi
                ISSUER_CLIENT_SECRET="$2"
                shift 2
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

validate_services() {
    local valid_services="agent registry worker"
    local IFS=','
    for svc in $SERVICES; do
        case " $valid_services " in
            *" $svc "*)
                ;;
            *)
                print_error "Invalid service: '$svc'" \
                    "Valid services are: agent, registry, worker" \
                    "Example: install.sh --services agent,registry,worker"
                exit 1
                ;;
        esac
    done
}

# Rejects a value that carries a character an installed unit/plist file
# could reinterpret as its own syntax (AC2). '$' and the backtick are on the
# list too, since either lets systemd or a shell re-expand the value. Used on
# the values an operator chooses and can retype: $ISSUER, $ISSUER_AUDIENCE
# and $ISSUER_CLIENT_ID, each of which is interpolated into a systemd
# `ExecStart=` line and a plist `ProgramArguments` array.
#
# Deliberately NOT used on $ISSUER_CLIENT_SECRET. That
# value is generated by the IdP, so refusing a character in it refuses the
# install outright with no action the operator can take; and it no longer
# crosses this boundary anyway — it goes into a systemd
# EnvironmentFile or a plist EnvironmentVariables entry, whose grammars
# `systemd_env_escape` and `xml_escape` encode for.
#
# '%' is deliberately NOT on this list: a legal RFC 3986
# percent-encoded octet in an issuer path or query (`%20`, a non-ASCII
# realm name IDNA/percent-encodes into) is text the CLI's own URL parser
# accepts, so refusing it here would make that value permanently
# uninstallable rather than merely unsafe. '%' is only meaningful to
# systemd's unit-file specifier expansion, not to plist XML, so it is
# escaped at the one call site that needs it (`systemd_specifier_escape`,
# applied to $ISSUER/$ISSUER_AUDIENCE/$ISSUER_CLIENT_ID where they are
# interpolated into `ExecStart=`) rather than refused everywhere.
#
# Args: $1 = human-readable field name (for the error message), $2 = value.
validate_no_unit_injection_chars() {
    local field="$1"
    local value="$2"

    case "$value" in
        *$'\n'* | *'"'* | *'<'* | *'>'* | *'&'* | *'\'* | *'$'* | *'`'*)
            print_error "Invalid $field: contains a disallowed character" \
                "$field values may not contain newlines, quotes, angle brackets, '&', '\$', backticks, or backslashes." \
                "This value is written into a systemd unit and a launchd plist; those characters could change what either file runs."
            exit 1
            ;;
    esac
}

# Escapes '%' as '%%' so a value survives systemd's unit-file specifier
# expansion literally: unescaped, a legal percent-encoded
# octet in an operator's issuer text (e.g. `%20`) would either be
# misinterpreted as a specifier systemd does understand, or — before this
# function existed — be refused outright by the denylist above, permanently,
# with no value the operator could supply instead. Doubling every '%'
# neutralizes both failure modes regardless of what follows it, because
# `%%` is systemd's own escape for a literal percent sign. Applied only at
# the `ExecStart=` interpolation site: plist `<string>` elements have no
# specifier syntax, so `%` needs no XML-side handling.
systemd_specifier_escape() {
    local value="$1"
    printf '%s' "${value//%/%%}"
}

# Escapes the five XML predefined entities so a value can sit inside a plist
# <string> element as text rather than as markup. The
# ampersand is substituted first, or it would double-escape the entities the
# later substitutions introduce.
xml_escape() {
    local value="$1"
    value=${value//&/\&amp;}
    value=${value//</\&lt;}
    value=${value//>/\&gt;}
    value=${value//\"/\&quot;}
    value=${value//\'/\&apos;}
    printf '%s' "$value"
}

# Encodes a value for a double-quoted assignment in a systemd
# EnvironmentFile. Inside double quotes systemd drops the
# backslash only before `"`, `\`, '$' and the backtick, and keeps BOTH
# characters for any other `\<char>` pair. So this list must stay exactly
# those four: escaping one more character emits it as a literal backslash
# plus the character and corrupts the value. Every other character is
# already carried verbatim between the quotes -- including the single
# quote, '#', and interior whitespace -- so none of them needs escaping.
# A newline is the one thing the line format cannot hold, and
# `require_representable_client_secret` refuses that separately.
#
# '$' and the backtick are escaped too, though systemd expands neither: an
# operator debugging a unit reaches for `source ~/.config/systemd/user/
# vorpal.env`, and a secret carrying either character would run as a command
# substitution in that shell. Escaping them costs nothing, because systemd
# strips the backslash before any consumer sees the value.
systemd_env_escape() {
    local value="$1"
    value=${value//\\/\\\\}
    value=${value//\"/\\\"}
    value=${value//\$/\\\$}
    value=${value//\`/\\\`}
    printf '%s' "$value"
}

# The one character of an IdP-generated client secret that no encoding here
# can carry: a systemd EnvironmentFile assignment is a
# single line. The remedy is at the IdP, so the message says so rather than
# telling the operator to edit a value that is not theirs to edit.
require_representable_client_secret() {
    case "$ISSUER_CLIENT_SECRET" in
        *$'\n'*)
            print_error "Invalid issuer client secret: contains a newline" \
                "The secret is stored as a single line in a systemd EnvironmentFile, which cannot represent a newline." \
                "Rotate the client secret at your identity provider, then re-run with the new value."
            exit 1
            ;;
    esac
}

# Rejects an $ISSUER that is not a well-formed https (or http+loopback)
# URL, or that carries a character an installed unit/plist file could
# reinterpret as its own syntax (AC2). Mirrors validate_services'
# allow-list shape rather than escaping: $ISSUER crosses into two different
# downstream grammars (systemd unit, launchd plist XML), and one allow-list
# is cheaper to get right than two escapers. The scheme, the authority and
# the host are taken apart explicitly rather than matched by prefix, so this
# accepts exactly what the CLI's `credential_egress_origin` accepts and
# nothing broader.
validate_issuer() {
    if [[ -z "$ISSUER" ]]; then
        return
    fi

    validate_no_unit_injection_chars "issuer" "$ISSUER"

    # Split off the authority before deciding anything, rather than matching
    # the whole URL by prefix. Two values the prefix form
    # accepted are exactly the two the CLI's `credential_egress_origin`
    # refuses: `https://` on its own, which has no host at all, and
    # `http://localhost:pw@evil.example/`, whose `localhost:` is *userinfo*
    # and whose host is off-box. Both would have installed a unit that can
    # never start; the second reads as a loopback issuer while pointing the
    # trust anchor at an attacker.
    local scheme authority host
    case "$ISSUER" in
        https://*)
            scheme=https
            authority=${ISSUER#https://}
            ;;
        http://*)
            scheme=http
            authority=${ISSUER#http://}
            ;;
        *)
            print_error "Invalid issuer: '$ISSUER'" \
                "The issuer must be an https URL (plaintext http is only allowed on localhost or 127.0.0.1)." \
                "Example: install.sh --issuer https://idp.example.com/realms/vorpal"
            exit 1
            ;;
    esac

    authority=${authority%%/*}

    case "$authority" in
        '' | *@*)
            print_error "Invalid issuer: '$ISSUER'" \
                "The issuer must name a host directly, with no user:password@ prefix." \
                "Example: install.sh --issuer https://idp.example.com/realms/vorpal"
            exit 1
            ;;
    esac

    # A bracketed IPv6 host has to be matched before the generic `*:*` arm:
    # that arm strips everything from the *first* colon, and the colons in
    # `[::1]` are inside the brackets, so it would leave `host='['` and
    # report a nonsense host in the refusal below.
    case "$authority" in
        \[*\]:*) host=${authority%%\]:*}\] ;;
        \[*\]) host=$authority ;;
        *:*) host=${authority%%:*} ;;
        *) host=$authority ;;
    esac

    if [[ "$scheme" = "http" ]]; then
        case "$host" in
            localhost | 127.0.0.1) ;;
            '[::1]')
                # Refused even though it is loopback, because the CLI refuses
                # it: `credential_egress_origin` (sdk/rust/src/context.rs)
                # compares `Url::host_str()` against the unbracketed "::1",
                # and the url crate always serializes an IPv6 host with its
                # brackets, so that comparison never matches. Installing this
                # value would write a unit that restart-loops. This script's
                # rule must stay a subset of the CLI's: refusing something the
                # CLI would accept costs an install, accepting something the
                # CLI refuses costs a service that can never start.
                print_error "Invalid issuer: '$ISSUER'" \
                    "A bracketed IPv6 loopback issuer is not supported yet: 'vorpal system services start' refuses it, so the installed service would not start." \
                    "Use http://localhost:PORT/... or http://127.0.0.1:PORT/... instead."
                exit 1
                ;;
            *)
                print_error "Invalid issuer: '$ISSUER'" \
                    "Plaintext http is only allowed on localhost or 127.0.0.1; '$host' is neither." \
                    "Example: install.sh --issuer https://idp.example.com/realms/vorpal"
                exit 1
                ;;
        esac
    fi
}

# Validates the two operator-chosen OIDC client values against the same
# character allow-list as $ISSUER: they cross the
# identical plist/systemd-unit boundary and were unvalidated even after AC2
# added validate_issuer for the issuer value alone. The client secret does
# not cross that boundary and is not the operator's to retype, so it gets
# the encode-and-accept treatment instead.
validate_issuer_client_values() {
    [[ -n "$ISSUER_AUDIENCE" ]] && validate_no_unit_injection_chars "issuer audience" "$ISSUER_AUDIENCE"
    [[ -n "$ISSUER_CLIENT_ID" ]] && validate_no_unit_injection_chars "issuer client ID" "$ISSUER_CLIENT_ID"
    [[ -n "$ISSUER_CLIENT_SECRET" ]] && require_representable_client_secret
    return 0
}

# Refuses to write a worker/registry unit that cannot start (AC2, AC4):
# `resolve_required_issuer` (cli/src/command/start.rs) refuses
# any worker or registry start with no --issuer, so an installed unit with
# no issuer would enter a restart loop instead of running. Naming the exact
# migration step here, before install_service writes anything, is cheaper
# than letting the operator find it in a service log (AC4).
require_issuer_for_authenticated_services() {
    if [[ -n "$ISSUER" ]]; then
        return
    fi

    local IFS=','
    for svc in $SERVICES; do
        case "$svc" in
            worker | registry)
                print_error "Missing --issuer for service '$svc'" \
                    "Starting a worker or registry service now requires an OIDC issuer; unauthenticated starts are refused." \
                    "Supply one: install.sh --issuer https://idp.example.com/realms/vorpal (or VORPAL_ISSUER=...). Unauthenticated installs are not supported yet."
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
    if [[ -n "$PENDING_WRITE_TMP" ]]; then
        rm -f "$PENDING_WRITE_TMP"
        PENDING_WRITE_TMP=""
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

install_linux_prerequisites() {
    print_header "Installing Linux prerequisites"

    local distro_id=""
    if [[ -f /etc/os-release ]]; then
        distro_id="$(. /etc/os-release 2>/dev/null && printf '%s' "${ID:-}")"
    fi

    if command -v apt-get >/dev/null 2>&1; then
        install_linux_prerequisites_apt
    elif [[ "$distro_id" = "fedora" ]] && command -v dnf >/dev/null 2>&1; then
        install_linux_prerequisites_dnf
    else
        print_error \
            "No supported package manager found" \
            "This installer supports apt-get (Debian/Ubuntu) and dnf (Fedora).
  Your system does not appear to have either." \
            "Install the following packages manually using your package manager:
    ${_sym_bullet} bubblewrap (provides bwrap)
    ${_sym_bullet} docker (see https://docs.docker.com/engine/install/)
  Then re-run the installer."
        exit 1
    fi
}

install_linux_prerequisites_apt() {
    local need_bwrap=0
    local need_docker=0

    if command -v bwrap >/dev/null 2>&1; then
        print_success "bubblewrap (already installed)"
    else
        need_bwrap=1
    fi

    if command -v docker >/dev/null 2>&1; then
        print_success "docker (already installed)"
    else
        need_docker=1
    fi

    local invoking_user
    invoking_user="$(id -un)"

    local need_docker_group=0
    if id -nG 2>/dev/null | tr ' ' '\n' | grep -qx docker; then
        print_success "docker group membership (already a member)"
    else
        need_docker_group=1
    fi

    if [[ "$need_bwrap" = 0 ]] && [[ "$need_docker" = 0 ]] && [[ "$need_docker_group" = 0 ]]; then
        return 0
    fi

    local packages=()
    if [[ "$need_bwrap" = 1 ]]; then
        packages+=("bubblewrap")
    fi
    if [[ "$need_docker" = 1 ]]; then
        packages+=("docker.io")
    fi

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        if [[ ${#packages[@]} -gt 0 ]]; then
            local pkg
            for pkg in "${packages[@]}"; do
                print_step "Would install ${pkg} via apt-get"
            done
        fi
        if [[ "$need_docker_group" = 1 ]]; then
            print_step "Would add ${invoking_user} to the docker group (grants root-equivalent access via the docker socket)"
        fi
        print_success "Linux prerequisites (dry run)"
        return 0
    fi

    if [[ ${#packages[@]} -gt 0 ]]; then
        print_warning "Vorpal needs to install ${packages[*]} (requires sudo)"

        spin "Updating package lists..."
        if ! sudo apt-get update -y >/dev/null 2>&1; then
            spin_stop "failure"
            print_error \
                "Failed to update package lists" \
                "apt-get update failed. This is required before installing packages." \
                "Options:
    ${_sym_bullet} Check your internet connection
    ${_sym_bullet} Run 'sudo apt-get update' manually and re-run the installer"
            exit 1
        fi
        spin_stop "success" "Updated package lists"

        if [[ "$need_bwrap" = 1 ]]; then
            spin "Installing bubblewrap..."
            if ! sudo apt-get install -y bubblewrap >/dev/null 2>&1; then
                spin_stop "failure"
                print_error \
                    "Failed to install bubblewrap" \
                    "Vorpal requires bubblewrap (bwrap) for sandboxed builds on Linux." \
                    "Options:
    ${_sym_bullet} Run 'sudo apt-get install -y bubblewrap' manually
    ${_sym_bullet} Check that your package sources include bubblewrap"
                exit 1
            fi
            spin_stop "success" "bubblewrap (installed)"
        fi

        if [[ "$need_docker" = 1 ]]; then
            spin "Installing docker..."
            if ! sudo apt-get install -y docker.io >/dev/null 2>&1; then
                spin_stop "failure"
                print_error \
                    "Failed to install docker" \
                    "Vorpal requires docker as a container runtime on Linux." \
                    "Options:
    ${_sym_bullet} Run 'sudo apt-get install -y docker.io' manually
    ${_sym_bullet} Install Docker from https://docs.docker.com/engine/install/"
                exit 1
            fi
            spin_stop "success" "docker (installed)"
        fi
    fi

    if [[ "$need_docker_group" = 1 ]]; then
        # docker-group membership grants root-equivalent access via the docker socket;
        # this is the accepted tradeoff for running docker without sudo.
        print_warning "Adding ${invoking_user} to the docker group grants root-equivalent access via the docker socket (requires sudo)"
        spin "Adding ${invoking_user} to the docker group..."
        if ! sudo usermod -aG docker -- "${invoking_user}" >/dev/null 2>&1; then
            spin_stop "failure"
            print_error \
                "Failed to add ${invoking_user} to the docker group" \
                "Vorpal needs the current user in the docker group to run docker without sudo." \
                "Options:
    ${_sym_bullet} Run 'sudo usermod -aG docker ${invoking_user}' manually
    ${_sym_bullet} Continue running docker commands with sudo"
            exit 1
        fi
        spin_stop "success" "Added ${invoking_user} to the docker group"
        print_warning "Log out and back in (or run 'newgrp docker') for docker group membership to take effect"
    fi
}

# dnf config-manager syntax differs between dnf4 (--add-repo <url>) and dnf5
# (addrepo --from-repofile=<url>). dnf5 additionally chokes on blank lines in
# Docker's published .repo file (rpm-software-management/dnf5#1603), so fetch
# it and strip blank lines first, matching Docker's own installer workaround.
add_docker_ce_dnf_repo() {
    local repo_url="https://download.docker.com/linux/fedora/docker-ce.repo"

    spin "Adding Docker CE repository..."

    if command -v dnf5 >/dev/null 2>&1; then
        local tmp_repo_file
        tmp_repo_file="$(mktemp)"
        if ! curl -fsSL "$repo_url" | tr -s '\n' > "$tmp_repo_file"; then
            rm -f "$tmp_repo_file"
            spin_stop "failure"
            return 1
        fi
        sudo dnf5 config-manager addrepo --save-filename=docker-ce.repo --overwrite --from-repofile="$tmp_repo_file" >/dev/null 2>&1
        local status=$?
        rm -f "$tmp_repo_file"
        if [[ "$status" -eq 0 ]]; then
            spin_stop "success" "Added Docker CE repository"
        else
            spin_stop "failure"
        fi
        return $status
    fi

    sudo dnf config-manager --add-repo "$repo_url" >/dev/null 2>&1
    local status=$?
    if [[ "$status" -eq 0 ]]; then
        spin_stop "success" "Added Docker CE repository"
    else
        spin_stop "failure"
    fi
    return $status
}

install_linux_prerequisites_dnf() {
    local need_bwrap=0
    local need_docker=0

    if command -v bwrap >/dev/null 2>&1; then
        print_success "bubblewrap (already installed)"
    else
        need_bwrap=1
    fi

    if command -v docker >/dev/null 2>&1; then
        print_success "docker (already installed)"
    else
        need_docker=1
    fi

    local invoking_user
    invoking_user="$(id -un)"

    local need_docker_group=0
    if id -nG 2>/dev/null | tr ' ' '\n' | grep -qx docker; then
        print_success "docker group membership (already a member)"
    else
        need_docker_group=1
    fi

    if [[ "$need_bwrap" = 0 ]] && [[ "$need_docker" = 0 ]] && [[ "$need_docker_group" = 0 ]]; then
        return 0
    fi

    local packages=()
    if [[ "$need_bwrap" = 1 ]]; then
        packages+=("bubblewrap")
    fi
    if [[ "$need_docker" = 1 ]]; then
        packages+=("docker-ce" "docker-ce-cli" "containerd.io")
    fi

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        if [[ "$need_bwrap" = 1 ]]; then
            print_step "Would install bubblewrap via dnf"
        fi
        if [[ "$need_docker" = 1 ]]; then
            print_step "Would install dnf-plugins-core via dnf (if needed)"
            print_step "Would add Docker CE repo via dnf config-manager"
            print_step "Would install docker-ce docker-ce-cli containerd.io via dnf"
            print_step "Would enable/start docker daemon (systemctl enable --now docker)"
        fi
        if [[ "$need_docker_group" = 1 ]]; then
            print_step "Would add ${invoking_user} to the docker group (grants root-equivalent access via the docker socket)"
        fi
        print_success "Linux prerequisites (dry run)"
        return 0
    fi

    if [[ ${#packages[@]} -gt 0 ]]; then
        local warn_msg="Vorpal needs to install ${packages[*]}"
        if [[ "$need_docker" = 1 ]]; then
            warn_msg="${warn_msg} and enable+start the docker service (runs as a persistent root daemon)"
        fi
        print_warning "${warn_msg} (requires sudo)"

        if [[ "$need_bwrap" = 1 ]]; then
            spin "Installing bubblewrap..."
            if ! sudo dnf install -y bubblewrap >/dev/null 2>&1; then
                spin_stop "failure"
                print_error \
                    "Failed to install bubblewrap" \
                    "Vorpal requires bubblewrap (bwrap) for sandboxed builds on Linux." \
                    "Options:
    ${_sym_bullet} Run 'sudo dnf install -y bubblewrap' manually
    ${_sym_bullet} Check that your package sources include bubblewrap"
                exit 1
            fi
            spin_stop "success" "bubblewrap (installed)"
        fi

        if [[ "$need_docker" = 1 ]]; then
            if [[ ! -f /etc/yum.repos.d/docker-ce.repo ]]; then
                spin "Installing dnf-plugins-core..."
                if ! sudo dnf install -y dnf-plugins-core >/dev/null 2>&1; then
                    spin_stop "failure"
                    print_error \
                        "Failed to install dnf-plugins-core" \
                        "The Docker CE dnf repo requires dnf-plugins-core (provides dnf config-manager)." \
                        "Options:
    ${_sym_bullet} Run 'sudo dnf install -y dnf-plugins-core' manually
    ${_sym_bullet} Check your internet connection"
                    exit 1
                fi
                spin_stop "success" "dnf-plugins-core (installed)"

                if ! add_docker_ce_dnf_repo; then
                    print_error \
                        "Failed to add Docker CE repo" \
                        "Could not add the official Docker CE dnf repo." \
                        "Options:
    ${_sym_bullet} Run 'sudo dnf config-manager --add-repo https://download.docker.com/linux/fedora/docker-ce.repo' manually (dnf5: 'sudo dnf5 config-manager addrepo --from-repofile=https://download.docker.com/linux/fedora/docker-ce.repo')
    ${_sym_bullet} Check your internet connection"
                    exit 1
                fi
            fi

            spin "Installing docker..."
            if ! sudo dnf install -y docker-ce docker-ce-cli containerd.io >/dev/null 2>&1; then
                spin_stop "failure"
                print_error \
                    "Failed to install docker" \
                    "Vorpal requires docker as a container runtime on Linux." \
                    "Options:
    ${_sym_bullet} Run 'sudo dnf install -y docker-ce docker-ce-cli containerd.io' manually
    ${_sym_bullet} Install Docker from https://docs.docker.com/engine/install/fedora/"
                exit 1
            fi
            spin_stop "success" "docker (installed)"

            spin "Enabling docker service..."
            if sudo systemctl enable --now docker >/dev/null 2>&1; then
                spin_stop "success" "Docker service enabled"
            else
                spin_stop "failure"
                print_warning "Docker installed but the service could not be started. Run manually: sudo systemctl enable --now docker"
            fi
        fi
    fi

    if [[ "$need_docker_group" = 1 ]]; then
        # docker-group membership grants root-equivalent access via the docker socket;
        # this is the accepted tradeoff for running docker without sudo.
        print_warning "Adding ${invoking_user} to the docker group grants root-equivalent access via the docker socket (requires sudo)"
        spin "Adding ${invoking_user} to the docker group..."
        if ! sudo usermod -aG docker -- "${invoking_user}" >/dev/null 2>&1; then
            spin_stop "failure"
            print_error \
                "Failed to add ${invoking_user} to the docker group" \
                "Vorpal needs the current user in the docker group to run docker without sudo." \
                "Options:
    ${_sym_bullet} Run 'sudo usermod -aG docker ${invoking_user}' manually
    ${_sym_bullet} Continue running docker commands with sudo"
            exit 1
        fi
        spin_stop "success" "Added ${invoking_user} to the docker group"
        print_warning "Log out and back in (or run 'newgrp docker') for docker group membership to take effect"
    fi
}

setup_apparmor_profile() {
    print_header "Setting up AppArmor profile for bubblewrap"

    local profile_path="/etc/apparmor.d/bwrap"

    if [[ -f "$profile_path" ]]; then
        print_success "AppArmor bwrap profile (already exists)"
        return 0
    fi

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        print_step "Would create ${profile_path} with bubblewrap AppArmor profile"
        print_success "AppArmor bwrap profile (dry run)"
        return 0
    fi

    print_warning "Vorpal needs to create ${profile_path} (requires sudo)"

    spin "Creating AppArmor profile..."
    if ! sudo tee "$profile_path" >/dev/null 2>&1 <<'APPARMOR_EOF'
abi <abi/4.0>,
include <tunables/global>

profile bwrap /usr/bin/bwrap flags=(unconfined) {
  userns,

  # Site-specific additions and overrides. See local/README for details.
  include if exists <local/bwrap>
}
APPARMOR_EOF
    then
        spin_stop "failure"
        print_error \
            "Failed to create AppArmor profile for bubblewrap" \
            "Vorpal needs an AppArmor profile at ${profile_path} to allow bubblewrap
  to use user namespaces." \
            "Options:
    ${_sym_bullet} Re-run and enter your password when prompted
    ${_sym_bullet} Ask your system administrator for sudo access
    ${_sym_bullet} Create the profile manually:
        sudo tee ${profile_path} with the bubblewrap profile content"
        exit 1
    fi
    spin_stop "success" "Created AppArmor profile"

    spin "Loading AppArmor profile..."
    if sudo apparmor_parser -r "$profile_path" 2>/dev/null; then
        spin_stop "success" "Loaded AppArmor profile"
    else
        spin_stop "failure"
        print_warning "AppArmor profile created but could not be loaded. A reboot may be required."
    fi

    print_success "AppArmor bwrap profile (created)"
}

# -- Phase 2: Download & version resolution -----------------------------------

resolve_version() {
    # Note: Version resolution runs during --dry-run to validate the plan is realistic.
    # This makes read-only HTTP requests (API query, HEAD validation).
    local version="$VORPAL_VERSION"
    local artifact="vorpal-${ARCH}-${OS}.tar.gz"

    if [[ "$version" = "latest" ]]; then
        # Query GitHub API for latest non-prerelease tag
        local api_url="https://api.github.com/repos/${VORPAL_REPO}/releases/latest"
        local api_response
        local http_code

        spin "Resolving latest version..."

        http_code="$(curl -sS -o /dev/null -w "%{http_code}" "$api_url" 2>/dev/null)" || true

        if [[ "$http_code" = "403" ]]; then
            spin_stop "failure"
            print_warning "GitHub API rate limit reached. Falling back to 0.4.1."
            version="0.4.1"
        elif [[ "$http_code" = "200" ]]; then
            api_response="$(curl -fsSL "$api_url" 2>/dev/null)" || true
            if [[ -n "$api_response" ]]; then
                # Extract tag_name — simple grep to avoid jq dependency
                local tag
                tag="$(printf '%s' "$api_response" | grep -o '"tag_name"[[:space:]]*:[[:space:]]*"[^"]*"' | head -1 | sed 's/.*"tag_name"[[:space:]]*:[[:space:]]*"//;s/"//')"
                if [[ -n "$tag" ]]; then
                    version="$tag"
                    spin_stop "success" "Resolved latest version (${tag})"
                else
                    spin_stop "failure"
                    print_warning "Could not parse latest version from GitHub API. Falling back to 0.4.1."
                    version="0.4.1"
                fi
            else
                spin_stop "failure"
                print_warning "Could not fetch latest version from GitHub API. Falling back to 0.4.1."
                version="0.4.1"
            fi
        else
            spin_stop "failure"
            print_warning "GitHub API returned HTTP ${http_code}. Falling back to 0.4.1."
            version="0.4.1"
        fi
    fi

    RESOLVED_VERSION="$version"
    DOWNLOAD_URL="${VORPAL_GITHUB_URL}/releases/download/${version}/${artifact}"

    # Validate version exists via HTTP HEAD
    spin "Verifying version ${version}..."

    local head_code
    head_code="$(curl -fsSI -o /dev/null -w "%{http_code}" "$DOWNLOAD_URL" 2>/dev/null)" || true

    if [[ "$head_code" != "200" ]] && [[ "$head_code" != "302" ]]; then
        spin_stop "failure"
        print_error \
            "Version \"${VORPAL_VERSION}\" not found." \
            "Available channels:
    ${_sym_bullet} nightly -- latest development build (updated daily)
    ${_sym_bullet} latest  -- most recent stable release" \
            "Or specify an exact tag: --version v0.4.1
  See all releases: ${VORPAL_GITHUB_URL}/releases"
        exit 1
    fi

    spin_stop "success" "Verified version ${version}"
}

handle_existing() {
    local binary="${VORPAL_INSTALL_DIR}/bin/vorpal"

    if [[ ! -x "$binary" ]]; then
        return 0
    fi

    # Get current installed version
    EXISTING_VERSION="$("$binary" --version 2>/dev/null || printf 'unknown')"

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        # Dry-run: show default choice without prompting
        IS_UPGRADE=1
        print_warning "Vorpal is already installed (${EXISTING_VERSION}). Would upgrade to ${RESOLVED_VERSION} (default)."
        return 0
    fi

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

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        print_step "Would download Vorpal ${RESOLVED_VERSION} from ${DOWNLOAD_URL}"
        print_step "Would extract to ${VORPAL_INSTALL_DIR}/bin/vorpal"
        print_step "Would verify binary"
        print_success "Download (dry run)"
        return 0
    fi

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

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        print_step "Would create ${system_dir}/{key,log,sandbox,store,...}"
        print_step "Would set ownership to current user (${current_uid}:${current_gid})"
        print_success "System directories (dry run)"
        return 0
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

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        print_step "Would generate security keys"
        print_success "Security keys (dry run)"
        return 0
    fi

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

# Writes (or removes) the systemd EnvironmentFile that carries VORPAL_ISSUER
# and, when configured, VORPAL_ISSUER_CLIENT_SECRET to the installed unit.
# The secret goes here rather than into ExecStart argv, which any local user
# can read from `ps`/`/proc/<pid>/cmdline`; a mode-600 file they cannot.
#
# Called before the unit is written so `EnvironmentFile=` is never dangling.
#
# Both writes are unconditional `if` blocks rather than `[[ … ]] && printf`.
# Under `set -e` the exit status of the *last* command in the group is the
# group's status, so a trailing conditional printf that does not fire aborted
# the installer — which is exactly the issuer-configured-without-a-secret
# case this migration path exists to serve. `validate_issuer_client_values`
# ends with an explicit `return 0` for the same reason.
#
# `cli/src/command.rs`'s installer tests execute this file to pin the
# behaviour below; keep the function name and its refusal messages in step
# with them.
write_service_env_file() {
    local env_path="$1"
    local env_tmp="${env_path}.tmp"

    if [[ -z "$ISSUER" && -z "$ISSUER_CLIENT_SECRET" ]]; then
        rm -f "$env_path"
        return 0
    fi

    # Create a fresh temporary sibling under a 077 umask and rename it into
    # place. umask governs a file's mode only at *creation*, so overwriting
    # an existing env file would write a new secret into whatever mode that
    # file already carried; and unlinking the live file first would leave
    # the unit with no EnvironmentFile at all if the write is interrupted.
    # `mv` within one directory is atomic, so the installed path only ever
    # holds a complete, mode-600 file.
    #
    # The temporary sibling is announced to `cleanup` first: between the
    # create and the rename it holds the same client secret the installed
    # file does, so an interrupt there must not leave a live credential at a
    # path the uninstaller never looks at.
    PENDING_WRITE_TMP="$env_tmp"

    (
        umask 077
        rm -f "$env_tmp"
        {
            if [[ -n "$ISSUER" ]]; then
                printf 'VORPAL_ISSUER="%s"\n' "$(systemd_env_escape "$ISSUER")"
            fi
            if [[ -n "$ISSUER_CLIENT_SECRET" ]]; then
                printf 'VORPAL_ISSUER_CLIENT_SECRET="%s"\n' "$(systemd_env_escape "$ISSUER_CLIENT_SECRET")"
            fi
        } > "$env_tmp"
    )

    mv -f "$env_tmp" "$env_path"
    PENDING_WRITE_TMP=""
}

install_service_macos() {
    local plist_dir="${HOME}/Library/LaunchAgents"
    local plist_path="${plist_dir}/com.altf4llc.vorpal.plist"
    local vorpal_bin="${VORPAL_INSTALL_DIR}/bin/vorpal"
    local gui_target="gui/$(id -u)"

    mkdir -p "$plist_dir"

    # Write the plist. The client secret (if any) travels through
    # EnvironmentVariables rather than ProgramArguments: argv is readable
    # by any local user via `ps`, an environment key is not. VORPAL_ISSUER
    # travels there too, alongside the existing --issuer on argv; argv still
    # wins on precedence (pinned by
    # `system_services_start_issuer_flag_takes_precedence_over_the_environment`
    # in cli/src/command.rs), so this only gives the operator a second,
    # `ps`-invisible view of what the running service was configured with.
    #
    # Every operator-supplied value in this document goes through
    # `xml_escape`, in both the ProgramArguments array and the
    # EnvironmentVariables dict. `validate_no_unit_injection_chars` already
    # refuses the XML metacharacters in every value except
    # `$ISSUER_CLIENT_SECRET`, for which `xml_escape` is the only control; the
    # point is that the plist grammar is encoded at the site that writes it,
    # so relaxing the deny-list later cannot silently turn a value into
    # markup. `$SERVICES` is exempt — `validate_services` allow-lists it
    # element by element against a closed set of three names.
    #
    # Written to a temporary sibling under a 077 umask and renamed into
    # place, never truncated or unlinked in situ. umask governs a file's
    # mode only at *creation*, so overwriting an existing plist would write
    # a new secret into whatever mode that file already had; and unlinking
    # the live plist first would leave the service with no plist at all if
    # the write is interrupted. Creating a fresh temporary file gets the
    # umask guarantee, and `mv` within one directory is atomic, so the
    # installed path only ever holds a complete file.
    #
    # The temporary sibling is announced to `cleanup` first: it carries the
    # same client secret the installed plist does, so an interrupt between
    # the create and the rename must not leave a live credential at a path
    # the uninstaller never looks at.
    local plist_tmp="${plist_path}.tmp"

    PENDING_WRITE_TMP="$plist_tmp"

    rm -f "$plist_tmp"

    local prior_umask
    prior_umask=$(umask)
    umask 077

    cat > "$plist_tmp" <<PLIST
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
        <string>--services</string>
        <string>${SERVICES}</string>$(
[[ -n "$ISSUER" ]] && printf '\n        <string>--issuer</string>\n        <string>%s</string>' "$(xml_escape "$ISSUER")"
[[ -n "$ISSUER_AUDIENCE" ]] && printf '\n        <string>--issuer-audience</string>\n        <string>%s</string>' "$(xml_escape "$ISSUER_AUDIENCE")"
[[ -n "$ISSUER_CLIENT_ID" ]] && printf '\n        <string>--issuer-client-id</string>\n        <string>%s</string>' "$(xml_escape "$ISSUER_CLIENT_ID")"
)
    </array>$(
if [[ -n "$ISSUER" || -n "$ISSUER_CLIENT_SECRET" ]]; then
    printf '\n    <key>EnvironmentVariables</key>\n    <dict>'
    [[ -n "$ISSUER" ]] && printf '\n        <key>VORPAL_ISSUER</key>\n        <string>%s</string>' "$(xml_escape "$ISSUER")"
    [[ -n "$ISSUER_CLIENT_SECRET" ]] && printf '\n        <key>VORPAL_ISSUER_CLIENT_SECRET</key>\n        <string>%s</string>' "$(xml_escape "$ISSUER_CLIENT_SECRET")"
    printf '\n    </dict>'
fi
)
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

    umask "$prior_umask"
    mv -f "$plist_tmp" "$plist_path"
    PENDING_WRITE_TMP=""

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
    local env_path="${unit_dir}/vorpal.env"
    local vorpal_bin="${VORPAL_INSTALL_DIR}/bin/vorpal"

    mkdir -p "$unit_dir"

    write_service_env_file "$env_path"

    # Write the systemd user unit through the same create-then-rename path as
    # the env file: the unit carries no secret today, but "umask governs
    # creation only" and "never unlink the live file" are the same two
    # invariants, and a future edit that puts anything sensitive in ExecStart
    # inherits them instead of having to rediscover them.
    local unit_tmp="${unit_path}.tmp"

    PENDING_WRITE_TMP="$unit_tmp"

    rm -f "$unit_tmp"

    local prior_umask
    prior_umask=$(umask)
    umask 077

    cat > "$unit_tmp" <<UNIT
[Unit]
Description=Vorpal Build System Services
After=network.target

[Service]
Type=simple
EnvironmentFile=-${env_path}
ExecStart=${vorpal_bin} system services start --services ${SERVICES}$(
[[ -n "$ISSUER" ]] && printf ' --issuer "%s"' "$(systemd_specifier_escape "$ISSUER")"
[[ -n "$ISSUER_AUDIENCE" ]] && printf ' --issuer-audience "%s"' "$(systemd_specifier_escape "$ISSUER_AUDIENCE")"
[[ -n "$ISSUER_CLIENT_ID" ]] && printf ' --issuer-client-id "%s"' "$(systemd_specifier_escape "$ISSUER_CLIENT_ID")"
)
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
UNIT

    umask "$prior_umask"
    mv -f "$unit_tmp" "$unit_path"
    PENDING_WRITE_TMP=""

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

    # Enable lingering so user services persist after logout
    if loginctl enable-linger 2>/dev/null; then
        print_success "Enabled login lingering (services persist after logout)"
    else
        print_warning "Could not enable lingering. Run manually: loginctl enable-linger"
    fi
}

install_service() {
    print_header "Starting services"

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        case "$OS" in
            darwin)
                print_step "Would install LaunchAgent (com.altf4llc.vorpal)"
                ;;
            linux)
                print_step "Would install systemd user service (vorpal.service)"
                ;;
        esac
        print_step "Would start services: ${SERVICES}"
        [[ -n "$ISSUER" ]] && print_step "Would pass --issuer ${ISSUER}"
        [[ -n "$ISSUER_AUDIENCE" ]] && print_step "Would pass --issuer-audience ${ISSUER_AUDIENCE}"
        [[ -n "$ISSUER_CLIENT_ID" ]] && print_step "Would pass --issuer-client-id ${ISSUER_CLIENT_ID}"
        [[ -n "$ISSUER_CLIENT_SECRET" ]] && print_step "Would pass --issuer-client-secret (set)"
        print_success "Services (dry run)"
        return 0
    fi

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
    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        print_step "Would verify services"
        return 0
    fi

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
        elif [[ "$FLAG_DRY_RUN" = 1 ]]; then
            print_step "Would add PATH to ${bash_rc/#$HOME/~}: ${path_line}"
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
        elif [[ "$FLAG_DRY_RUN" = 1 ]]; then
            print_step "Would add PATH to ~/.zshrc: ${path_line}"
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
        elif [[ "$FLAG_DRY_RUN" = 1 ]]; then
            print_step "Would add PATH to ~/.config/fish/config.fish: ${fish_path_line}"
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

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
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

    if [[ "$FLAG_DRY_RUN" = 1 ]]; then
        printf '\n  %s%sDry run complete. No changes were made.%s\n' "${_fmt_bold}" "${_fmt_yellow}" "${_fmt_reset}"
        printf '\n'
        printf '  Run without --dry-run to install Vorpal %s.\n' "$RESOLVED_VERSION"
        printf '\n'
        printf '  Docs:     %s%s%s\n' "${_fmt_cyan}" "${VORPAL_GITHUB_URL}" "${_fmt_reset}"
        printf '  Issues:   %s%s/issues%s\n' "${_fmt_cyan}" "${VORPAL_GITHUB_URL}" "${_fmt_reset}"
        return 0
    fi

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
        printf '    %s Credentials:  stored OIDC client secret\n' "${_sym_bullet}"
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
        # The `.tmp` sibling goes with it: the plist writer creates it, and an
        # install interrupted between the create and the rename leaves it
        # holding the OIDC client secret. An uninstall that reports success
        # while a live credential survives is the posture this removal exists
        # to prevent, whichever path the credential is sitting at.
        rm -f "${plist_path}.tmp"
        if [[ -f "$plist_path" ]]; then
            rm -f "$plist_path"
            removed+=("LaunchAgent configuration")
        fi
    else
        systemctl --user stop vorpal.service 2>/dev/null || true
        systemctl --user disable vorpal.service 2>/dev/null || true
        local unit_path="${HOME}/.config/systemd/user/vorpal.service"
        rm -f "${unit_path}.tmp"
        if [[ -f "$unit_path" ]]; then
            rm -f "$unit_path"
            systemctl --user daemon-reload 2>/dev/null || true
            removed+=("systemd user service")
        fi

        # The EnvironmentFile holds the OIDC client secret, so it is removed
        # unconditionally and named in the summary: it
        # is a separate file from the unit and can outlive it, and an
        # uninstall that reports success while leaving a live IdP credential
        # on disk is a worse posture than the argv the secret came from. The
        # `.tmp` sibling the writer renames from is removed for the same
        # reason: an interrupted install leaves the secret there too.
        local env_path="${HOME}/.config/systemd/user/vorpal.env"
        rm -f "${env_path}.tmp"
        if [[ -f "$env_path" ]]; then
            rm -f "$env_path"
            removed+=("systemd credentials file (OIDC client secret)")
        fi
    fi

    # 2. Remove ~/.vorpal/
    if [[ -d "$VORPAL_INSTALL_DIR" ]]; then
        rm -rf "$VORPAL_INSTALL_DIR"
        removed+=("${VORPAL_INSTALL_DIR/#$HOME/~}/")
    fi

    # 3. Remove /var/lib/vorpal/ (requires sudo)
    if [[ -d "$VORPAL_SYSTEM_DIR" ]]; then
        spin "Removing ${VORPAL_SYSTEM_DIR}..."
        if sudo rm -rf "$VORPAL_SYSTEM_DIR" 2>/dev/null; then
            spin_stop "success" "Removed ${VORPAL_SYSTEM_DIR}/"
            removed+=("$VORPAL_SYSTEM_DIR/")
        else
            spin_stop "failure"
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

    if [[ "$FLAG_DRY_RUN" = 1 ]] && [[ "$FLAG_UNINSTALL" = 1 ]]; then
        print_error "Cannot use --dry-run with --uninstall" \
            "These flags are mutually exclusive."
        exit 1
    fi

    if [[ "$FLAG_UNINSTALL" = 1 ]]; then
        run_uninstall
        exit 0
    fi

    # Validate before anything is downloaded or written:
    # these checks used to run after download_binary/setup_system_dirs/
    # generate_keys, so a malformed --issuer or a service/issuer mismatch
    # was only discovered after the release was fetched and system
    # directories and keys already existed.
    if [[ "$NO_SERVICE" != 1 ]]; then
        validate_services
        validate_issuer
        validate_issuer_client_values
        require_issuer_for_authenticated_services
    fi

    print_banner
    check_prerequisites

    if [[ "$OS" = "linux" ]]; then
        install_linux_prerequisites
        if command -v apparmor_parser >/dev/null 2>&1 || [[ -d /etc/apparmor.d ]]; then
            setup_apparmor_profile
        fi
    fi

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

# Run only when executed, not when sourced. The installer's validators and
# file writers are security controls with no shell test harness to reach
# them; sourcing this file makes them callable, which is how
# `cli/src/command.rs`'s installer tests drive them.
#
# `${BASH_SOURCE[0]:-$0}`, not `${BASH_SOURCE[0]}`: the documented install is
# `curl … | bash`, where the script arrives on stdin and BASH_SOURCE is empty,
# so under this file's `set -u` a bare array read aborts with "unbound
# variable" before main ever runs. Defaulting to `$0` makes the comparison
# true there, which is correct — a piped script is being executed, not
# sourced. `${BASH_SOURCE[0]:-}` would be the opposite bug: it compares empty
# against `$0` ("bash"), so `curl … | bash` would exit 0 having installed
# nothing.
if [[ "${BASH_SOURCE[0]:-$0}" == "$0" ]]; then
    main "$@"
fi
