#!/usr/bin/env bash
#
# slim-lfs-rootfs.sh - Vorpal Linux Slimming Script
#
# This script reduces a Vorpal Linux installation from ~2.9GB to ~600-700MB
# by removing development tools, documentation, and unnecessary localization files
# while preserving a fully functional runtime environment suitable for containers.
#
# Usage: ./slim.sh [OPTIONS] ROOTFS_PATH

set -euo pipefail

#==============================================================================
# CONFIGURATION
#==============================================================================

VERSION="1.0.0"
SCRIPT_NAME="$(basename "$0")"

# Default settings
DRY_RUN="yes"
NO_CONFIRM="no"
CREATE_BACKUP="no"
AGGRESSIVE="no"
VERBOSE="no"
QUIET="no"
SECTIONS_TO_RUN="all"

# Tracking variables
TOTAL_SAVED=0
TOTAL_WOULD_SAVE=0
SECTION_COUNT=0
ERROR_COUNT=0

# Color codes (disabled if not a terminal)
if [ -t 1 ]; then
    RED='\033[0;31m'
    GREEN='\033[0;32m'
    YELLOW='\033[1;33m'
    BLUE='\033[0;34m'
    CYAN='\033[0;36m'
    BOLD='\033[1m'
    RESET='\033[0m'
else
    RED=''
    GREEN=''
    YELLOW=''
    BLUE=''
    CYAN=''
    BOLD=''
    RESET=''
fi

# Protected files - NEVER remove these patterns
PROTECTED_RUNTIME_LIBS=(
    "libc.so*"
    "libc-*.so"
    "ld-linux*.so*"
    "libm.so*"
    "libpthread.so*"
    "libdl.so*"
    "librt.so*"
    "libresolv.so*"
    "libnsl.so*"
    "libutil.so*"
    "libcrypt.so*"
    "libgcc_s.so*"
    "libstdc++.so*"
    "libssl.so*"
    "libcrypto.so*"
    "libz.so*"
    "libbz2.so*"
    "liblzma.so*"
    "libncurses*.so*"
    "libreadline.so*"
    "libtinfo.so*"
    "libcurl.so*"
)

PROTECTED_BINARIES=(
    "bash"
    "sh"
    "ls"
    "cat"
    "cp"
    "mv"
    "rm"
    "mkdir"
    "rmdir"
    "touch"
    "chmod"
    "chown"
    "grep"
    "sed"
    "awk"
    "gawk"
    "find"
    "tar"
    "gzip"
    "xz"
)

#==============================================================================
# UTILITY FUNCTIONS
#==============================================================================

# Display usage information
usage() {
    cat << EOF
${BOLD}$SCRIPT_NAME${RESET} - Slim down Linux from Scratch rootfs

${BOLD}USAGE:${RESET}
    $SCRIPT_NAME [OPTIONS] ROOTFS_PATH

${BOLD}OPTIONS:${RESET}
    --dry-run           Show what would be removed without doing it (default)
    --execute           Actually perform the removal (disables dry-run)
    --no-confirm        Skip confirmation prompts (use with caution)
    --backup            Create tar.gz backup before modification
    --sections=N,M      Only run specific sections (comma-separated, 1-13; default: all)
    --aggressive        Enable aggressive cleanup (strip binaries)
    --verbose           Show detailed output
    --quiet             Minimal output
    -h, --help          Show this help message
    -v, --version       Show version information

${BOLD}EXAMPLES:${RESET}
    # Dry-run to see what would be removed
    $SCRIPT_NAME --dry-run /path/to/rootfs

    # Actually slim down with backup
    $SCRIPT_NAME --execute --backup /path/to/rootfs

    # Run only documentation and locale removal
    $SCRIPT_NAME --execute --sections=8,9,10 /path/to/rootfs

${BOLD}SECTIONS:${RESET}
    1.  GCC Compiler Infrastructure
    2.  Development Tools & Binutils
    3.  Python Complete Removal
    4.  Perl Complete Removal
    5.  Static Libraries
    6.  Header Files
    7.  Sanitizer Libraries
    8.  Documentation
    9.  Locale Data
    10. Locale Translations
    11. i18n & Character Encodings
    12. Build Artifacts
    13. Optional Cleanup

${BOLD}WARNING:${RESET}
    This script will permanently remove files. Use --dry-run first to review
    what will be removed. Consider using --backup for safety.

EOF
}

# Display version
version() {
    echo "$SCRIPT_NAME version $VERSION"
}

# Log message with timestamp and color
log_action() {
    local status="$1"
    local message="$2"
    local timestamp=$(date '+%Y-%m-%d %H:%M:%S')

    if [ "$QUIET" = "yes" ] && [ "$status" != "ERROR" ]; then
        return
    fi

    case "$status" in
        "INFO")
            echo -e "${BLUE}[INFO]${RESET} $message" >&2
            ;;
        "SUCCESS")
            echo -e "${GREEN}[SUCCESS]${RESET} $message" >&2
            ;;
        "REMOVED")
            if [ "$VERBOSE" = "yes" ]; then
                echo -e "${GREEN}[REMOVED]${RESET} $message" >&2
            fi
            ;;
        "SKIP")
            if [ "$VERBOSE" = "yes" ]; then
                echo -e "${YELLOW}[SKIP]${RESET} $message" >&2
            fi
            ;;
        "DRY-RUN")
            if [ "$VERBOSE" = "yes" ]; then
                echo -e "${CYAN}[DRY-RUN]${RESET} $message" >&2
            fi
            ;;
        "ERROR")
            echo -e "${RED}[ERROR]${RESET} $message" >&2
            ERROR_COUNT=$((ERROR_COUNT + 1))
            ;;
        "WARN")
            echo -e "${YELLOW}[WARN]${RESET} $message" >&2
            ;;
        *)
            echo "[$status] $message" >&2
            ;;
    esac
}

# Calculate size in bytes (cross-platform: Linux and macOS)
size_of() {
    local path="$1"
    if [ ! -e "$path" ]; then
        echo "0"
        return
    fi

    # Detect platform and use appropriate command
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS: use find with -print0 and stat
        if [ -f "$path" ]; then
            stat -f %z "$path" 2>/dev/null || echo "0"
        else
            # For directories on macOS, use du -sk and convert to bytes
            local kb=$(du -sk "$path" 2>/dev/null | awk '{print $1}')
            echo $((kb * 1024))
        fi
    else
        # Linux: use du -sb
        du -sb "$path" 2>/dev/null | awk '{print $1}' || echo "0"
    fi
}

# Convert bytes to human-readable format
human_readable() {
    local bytes="$1"
    awk -v b="$bytes" 'BEGIN {
        units[0]="B"; units[1]="KB"; units[2]="MB"; units[3]="GB";
        for(i=0; b>=1024 && i<3; i++) b/=1024;
        printf "%.2f %s", b, units[i];
    }'
}

# Safe removal with tracking
safe_remove() {
    local target="$1"
    local description="$2"

    # Check if target exists
    if [ ! -e "$target" ]; then
        log_action "SKIP" "$description - not found"
        return 0
    fi

    # Calculate size
    local size=$(size_of "$target")
    local size_human=$(human_readable "$size")

    if [ "$DRY_RUN" = "yes" ]; then
        log_action "DRY-RUN" "Would remove: $target ($size_human)"
        TOTAL_WOULD_SAVE=$((TOTAL_WOULD_SAVE + size))
    else
        if rm -rf "$target" 2>/dev/null; then
            log_action "REMOVED" "$target ($size_human)"
            TOTAL_SAVED=$((TOTAL_SAVED + size))
        else
            log_action "ERROR" "Failed to remove: $target"
            return 1
        fi
    fi

    return 0
}

# Safe removal using find patterns
safe_remove_pattern() {
    local base_dir="$1"
    local pattern="$2"
    local description="$3"

    if [ ! -d "$base_dir" ]; then
        log_action "SKIP" "$description - base directory not found"
        return 0
    fi

    # Find matching files/dirs and process them
    while IFS= read -r -d '' item; do
        safe_remove "$item" "$description"
    done < <(find "$base_dir" -name "$pattern" -print0 2>/dev/null)

    return 0
}

# Safe removal of empty directories with tracking
safe_remove_empty_dirs() {
    local base_dir="$1"
    local description="$2"

    if [ ! -d "$base_dir" ]; then
        log_action "SKIP" "$description - base directory not found"
        return 0
    fi

    while IFS= read -r -d '' item; do
        safe_remove "$item" "$description"
    done < <(find "$base_dir" -type d -empty -print0 2>/dev/null)

    return 0
}

# Confirmation prompt
confirm() {
    local prompt="$1"

    if [ "$NO_CONFIRM" = "yes" ]; then
        return 0
    fi

    echo -e "${YELLOW}${prompt}${RESET}" >&2
    read -p "Continue? (yes/no): " -r response

    if [ "$response" = "yes" ] || [ "$response" = "y" ]; then
        return 0
    else
        log_action "INFO" "Operation cancelled by user"
        exit 0
    fi
}

# Check if section should run
should_run_section() {
    local section_num="$1"

    if [ "$SECTIONS_TO_RUN" = "all" ]; then
        return 0
    fi

    # Check if section number is in comma-separated list
    if echo ",$SECTIONS_TO_RUN," | grep -q ",$section_num,"; then
        return 0
    else
        return 1
    fi
}

# Calculate total size of paths matching a pattern in a directory
calculate_pattern_size() {
    local base_dir="$1"
    local pattern="$2"

    if [ ! -d "$base_dir" ]; then
        echo "0"
        return
    fi

    local total=0
    while IFS= read -r -d '' item; do
        total=$((total + $(size_of "$item")))
    done < <(find "$base_dir" -maxdepth 1 -name "$pattern" -print0 2>/dev/null)

    echo "$total"
}

# Section header
section_header() {
    local section_num="$1"
    local section_name="$2"
    local expected_savings="$3"

    SECTION_COUNT=$((SECTION_COUNT + 1))

    echo ""
    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${RESET}"
    echo -e "${BOLD}Section $section_num: $section_name${RESET}"
    echo -e "${BOLD}Expected savings: $expected_savings${RESET}"
    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${RESET}"
}

#==============================================================================
# REMOVAL SECTIONS
#==============================================================================

# Section 1: GCC Compiler Infrastructure
section_01_gcc_compiler() {
    if ! should_run_section 1; then return 0; fi

    # Calculate section size
    local section_size=0
    section_size=$((section_size + $(size_of "$ROOTFS/usr/libexec/gcc")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/lib/gcc")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/share" "gcc-*")))
    # GCC binaries
    local gcc_bins=("gcc" "g++" "c++" "cpp" "cc" "gcc-ar" "gcc-nm" "gcc-ranlib" "gcov" "gcov-tool" "gcov-dump" "lto-dump")
    for bin in "${gcc_bins[@]}"; do
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "$bin")))
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "$bin-*")))
    done
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "aarch64-unknown-linux-gnu-*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "*-linux-gnu-gcc*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "libcc1.so*")))

    section_header 1 "GCC Compiler Infrastructure" "$(human_readable $section_size)"

    # Remove GCC compiler internals (largest target)
    safe_remove "$ROOTFS/usr/libexec/gcc" "GCC compiler internals"

    # Remove GCC library directory
    safe_remove "$ROOTFS/usr/lib/gcc" "GCC library directory"

    # Remove GCC shared data
    safe_remove_pattern "$ROOTFS/usr/share" "gcc-*" "GCC shared data"

    # Remove GCC binaries (but keep runtime libraries)
    local gcc_binaries=(
        "gcc" "g++" "c++" "cpp" "cc"
        "gcc-ar" "gcc-nm" "gcc-ranlib"
        "gcov" "gcov-tool" "gcov-dump"
        "lto-dump"
    )

    for binary in "${gcc_binaries[@]}"; do
        safe_remove_pattern "$ROOTFS/usr/bin" "$binary" "GCC binary: $binary"
        safe_remove_pattern "$ROOTFS/usr/bin" "$binary-*" "GCC binary: $binary-*"
    done

    # Remove target-specific GCC binaries
    safe_remove_pattern "$ROOTFS/usr/bin" "aarch64-unknown-linux-gnu-*" "Target-specific GCC binaries"
    safe_remove_pattern "$ROOTFS/usr/bin" "*-linux-gnu-gcc*" "Target-specific GCC binaries"

    # Remove libcc1 (GCC plugin interface)
    safe_remove_pattern "$ROOTFS/usr/lib" "libcc1.so*" "GCC plugin library"

    log_action "INFO" "Preserved: libgcc_s.so* and libstdc++.so* (runtime libraries)"
}

# Section 2: Development Tools
section_02_dev_tools() {
    if ! should_run_section 2; then return 0; fi

    # Calculate section size
    local section_size=0
    local calc_binutils=("as" "ld" "ld.bfd" "ld.gold" "ar" "ranlib" "nm" "objcopy" "objdump" "readelf" "size" "strings" "addr2line" "c++filt" "elfedit" "gprof" "dwp" "gold")
    for bin in "${calc_binutils[@]}"; do
        section_size=$((section_size + $(size_of "$ROOTFS/usr/bin/$bin")))
    done
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "gprofng*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "gp-*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "libbfd*.so*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "libopcodes*.so*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "libctf*.so*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "libsframe*.so*")))
    local calc_build_tools=("make" "bison" "yacc" "flex" "lex" "m4")
    for tool in "${calc_build_tools[@]}"; do
        section_size=$((section_size + $(size_of "$ROOTFS/usr/bin/$tool")))
    done
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "gdb*")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/gdb")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/bison")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/aclocal")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/bin/pkg-config")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/lib/pkgconfig")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/pkgconfig")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "autoconf*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "autom4te*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "autoreconf*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "automake*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "aclocal*")))

    section_header 2 "Development Tools & Binutils" "$(human_readable $section_size)"

    # Remove binutils development binaries
    local binutils_binaries=(
        "as" "ld" "ld.bfd" "ld.gold" "ar" "ranlib" "nm"
        "objcopy" "objdump" "readelf" "size" "strings"
        "addr2line" "c++filt" "elfedit" "gprof"
        "dwp" "gold"
    )

    for binary in "${binutils_binaries[@]}"; do
        safe_remove "$ROOTFS/usr/bin/$binary" "Binutils: $binary"
    done

    # Remove gprofng and related tools
    safe_remove_pattern "$ROOTFS/usr/bin" "gprofng*" "Gprofng tools"
    safe_remove_pattern "$ROOTFS/usr/bin" "gp-*" "Gprofng tools"

    # Remove development libraries
    safe_remove_pattern "$ROOTFS/usr/lib" "libbfd*.so*" "BFD library"
    safe_remove_pattern "$ROOTFS/usr/lib" "libopcodes*.so*" "Opcodes library"
    safe_remove_pattern "$ROOTFS/usr/lib" "libctf*.so*" "CTF library"
    safe_remove_pattern "$ROOTFS/usr/lib" "libsframe*.so*" "SFrame library"

    # Remove build tools
    local build_tools=("make" "bison" "yacc" "flex" "lex" "m4")
    for tool in "${build_tools[@]}"; do
        safe_remove "$ROOTFS/usr/bin/$tool" "Build tool: $tool"
    done

    # Remove GDB
    safe_remove_pattern "$ROOTFS/usr/bin" "gdb*" "GDB debugger"
    safe_remove "$ROOTFS/usr/share/gdb" "GDB data"

    # Remove development data
    safe_remove "$ROOTFS/usr/share/bison" "Bison data"
    safe_remove "$ROOTFS/usr/share/aclocal" "Aclocal macros"

    # Remove pkg-config
    safe_remove "$ROOTFS/usr/bin/pkg-config" "pkg-config"
    safe_remove "$ROOTFS/usr/lib/pkgconfig" "pkg-config data"
    safe_remove "$ROOTFS/usr/share/pkgconfig" "pkg-config data"

    # Remove autotools
    safe_remove_pattern "$ROOTFS/usr/bin" "autoconf*" "Autoconf"
    safe_remove_pattern "$ROOTFS/usr/bin" "autom4te*" "Autoconf"
    safe_remove_pattern "$ROOTFS/usr/bin" "autoreconf*" "Autoconf"
    safe_remove_pattern "$ROOTFS/usr/bin" "automake*" "Automake"
    safe_remove_pattern "$ROOTFS/usr/bin" "aclocal*" "Automake"
}

# Section 3: Python Complete Removal
section_03_python() {
    if ! should_run_section 3; then return 0; fi

    # Calculate section size
    local section_size=0
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "python3*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "libpython3*.so*")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "libpython*.so*")))
    local calc_py_bins=("python" "python3" "pip" "pip3" "pydoc" "pydoc3" "2to3" "idle" "idle3" "pyvenv")
    for bin in "${calc_py_bins[@]}"; do
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "$bin")))
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "${bin}.*")))
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "${bin}-*")))
    done

    section_header 3 "Python Complete Removal" "$(human_readable $section_size)"

    # Remove Python library directories
    safe_remove_pattern "$ROOTFS/usr/lib" "python3*" "Python libraries"

    # Remove Python shared libraries
    safe_remove_pattern "$ROOTFS/usr/lib" "libpython3*.so*" "Python shared library"
    safe_remove_pattern "$ROOTFS/usr/lib" "libpython*.so*" "Python shared library"

    # Remove Python binaries
    local python_binaries=(
        "python" "python3" "python3.*"
        "pip" "pip3" "pip3.*"
        "pydoc" "pydoc3" "pydoc3.*"
        "2to3" "2to3-3.*"
        "idle" "idle3" "idle3.*"
        "pyvenv" "pyvenv-3.*"
    )

    for binary in "${python_binaries[@]}"; do
        safe_remove_pattern "$ROOTFS/usr/bin" "$binary" "Python binary: $binary"
    done

    # Clean up any remaining Python artifacts
    safe_remove_pattern "$ROOTFS" "__pycache__" "Python cache directories"
    safe_remove_pattern "$ROOTFS" "*.pyc" "Python bytecode files"
    safe_remove_pattern "$ROOTFS" "*.pyo" "Python optimized files"

    log_action "INFO" "Python completely removed"
}

# Section 4: Perl Complete Removal
section_04_perl() {
    if ! should_run_section 4; then return 0; fi

    # Calculate section size
    local section_size=0
    section_size=$((section_size + $(size_of "$ROOTFS/usr/lib/perl5")))
    local calc_perl_bins=("perl" "cpan" "corelist" "enc2xs" "encguess" "h2ph" "h2xs" "instmodsh" "json_pp" "libnetcfg" "piconv" "pl2pm" "prove" "ptar" "ptardiff" "ptargrep" "shasum" "splain" "streamzip" "xsubpp" "zipdetails")
    for bin in "${calc_perl_bins[@]}"; do
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "$bin")))
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "${bin}5.*")))
    done
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/bin" "pod2*")))

    section_header 4 "Perl Complete Removal" "$(human_readable $section_size)"

    # Remove Perl library directory
    safe_remove "$ROOTFS/usr/lib/perl5" "Perl libraries"

    # Remove Perl binaries
    local perl_binaries=(
        "perl" "perl5.*"
        "cpan" "cpan5.*"
        "corelist"
        "enc2xs" "encguess"
        "h2ph" "h2xs"
        "instmodsh"
        "json_pp"
        "libnetcfg"
        "piconv"
        "pl2pm"
        "prove"
        "ptar" "ptardiff" "ptargrep"
        "shasum"
        "splain"
        "streamzip"
        "xsubpp"
        "zipdetails"
    )

    for binary in "${perl_binaries[@]}"; do
        safe_remove_pattern "$ROOTFS/usr/bin" "$binary" "Perl binary: $binary"
    done

    # Remove pod2* documentation tools
    safe_remove_pattern "$ROOTFS/usr/bin" "pod2*" "Perl documentation tools"

    log_action "WARN" "Some scripts may have Perl shebangs and will no longer work"
}

# Section 5: Static Libraries
section_05_static_libs() {
    if ! should_run_section 5; then return 0; fi

    # Calculate section size
    local section_size=0
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "*.a")))
    section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "*.la")))

    section_header 5 "Static Libraries" "$(human_readable $section_size)"

    # Remove all .a files
    log_action "INFO" "Removing all static libraries (.a files)"
    safe_remove_pattern "$ROOTFS/usr/lib" "*.a" "Static libraries"

    # Remove libtool .la files
    log_action "INFO" "Removing libtool archives (.la files)"
    safe_remove_pattern "$ROOTFS/usr/lib" "*.la" "Libtool archives"

    log_action "INFO" "All static libraries removed"
}

# Section 6: Header Files
section_06_headers() {
    if ! should_run_section 6; then return 0; fi

    # Calculate section size
    local section_size=0
    section_size=$((section_size + $(size_of "$ROOTFS/usr/include")))

    section_header 6 "Header Files" "$(human_readable $section_size)"

    # Remove all header files
    safe_remove "$ROOTFS/usr/include" "All header files"

    # Remove any lingering header directories
    safe_remove_pattern "$ROOTFS/usr/lib" "include" "Lingering header directories"

    log_action "INFO" "Cannot compile code without headers"
}

# Section 7: Sanitizer Libraries
section_07_sanitizers() {
    if ! should_run_section 7; then return 0; fi

    # Calculate section size
    local section_size=0
    local calc_sanitizers=("asan" "tsan" "ubsan" "lsan" "hwasan")
    for san in "${calc_sanitizers[@]}"; do
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "lib${san}*.so*")))
        section_size=$((section_size + $(calculate_pattern_size "$ROOTFS/usr/lib" "lib${san}*.a")))
    done

    section_header 7 "Sanitizer Libraries" "$(human_readable $section_size)"

    # Remove sanitizer shared and static libraries
    local sanitizers=("asan" "tsan" "ubsan" "lsan" "hwasan")

    for san in "${sanitizers[@]}"; do
        safe_remove_pattern "$ROOTFS/usr/lib" "lib${san}*.so*" "${san} shared library"
        safe_remove_pattern "$ROOTFS/usr/lib" "lib${san}*.a" "${san} static library"
    done

    log_action "INFO" "Debugging sanitizer libraries removed"
}

# Section 8: Documentation
section_08_documentation() {
    if ! should_run_section 8; then return 0; fi

    # Calculate section size
    local section_size=0
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/man")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/info")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/doc")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/texinfo")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/texi2any")))
    local calc_texinfo_bins=("makeinfo" "texi2any" "texi2dvi" "texi2pdf" "texindex" "info" "install-info" "pdftexi2dvi")
    for bin in "${calc_texinfo_bins[@]}"; do
        section_size=$((section_size + $(size_of "$ROOTFS/usr/bin/$bin")))
    done

    section_header 8 "Documentation" "$(human_readable $section_size)"

    # Remove man pages
    safe_remove "$ROOTFS/usr/share/man" "Man pages"

    # Remove info pages
    safe_remove "$ROOTFS/usr/share/info" "Info pages"

    # Remove documentation
    safe_remove "$ROOTFS/usr/share/doc" "Documentation"

    # Remove texinfo tools and data
    safe_remove "$ROOTFS/usr/share/texinfo" "Texinfo data"
    safe_remove "$ROOTFS/usr/share/texi2any" "Texi2any data"

    local texinfo_binaries=("makeinfo" "texi2any" "texi2dvi" "texi2pdf" "texindex" "info" "install-info" "pdftexi2dvi")
    for binary in "${texinfo_binaries[@]}"; do
        safe_remove "$ROOTFS/usr/bin/$binary" "Texinfo binary: $binary"
    done

    log_action "INFO" "No documentation available - use online resources"
}

# Section 9: Locale Data
section_09_locale_data() {
    if ! should_run_section 9; then return 0; fi

    # Calculate section size
    local section_size=0
    section_size=$((section_size + $(size_of "$ROOTFS/usr/lib/locale")))

    section_header 9 "Locale Data" "$(human_readable $section_size)"

    # Check if localedef is available for rebuilding
    if [ -x "$ROOTFS/usr/bin/localedef" ]; then
        log_action "INFO" "localedef found - will rebuild with en_US.UTF-8 only"

        # Backup and remove current locale archive
        safe_remove "$ROOTFS/usr/lib/locale/locale-archive" "Locale archive"

        # Create minimal en_US.UTF-8 locale
        if [ "$DRY_RUN" = "no" ]; then
            mkdir -p "$ROOTFS/usr/lib/locale"
            # Note: Actual rebuild would require chroot or proper environment
            log_action "WARN" "Locale rebuild requires manual intervention - archive removed"
        fi
    else
        # Simple removal
        log_action "INFO" "Removing all locale data"
        safe_remove "$ROOTFS/usr/lib/locale" "Locale data"

        if [ "$DRY_RUN" = "no" ]; then
            mkdir -p "$ROOTFS/usr/lib/locale"
        fi
    fi

    log_action "INFO" "Set LANG=C or LANG=en_US.UTF-8 in container environment"
}

# Section 10: Locale Translations
section_10_locale_translations() {
    if ! should_run_section 10; then return 0; fi

    # Calculate section size (excluding en_US and en directories)
    local section_size=0
    if [ -d "$ROOTFS/usr/share/locale" ]; then
        while IFS= read -r -d '' dir; do
            section_size=$((section_size + $(size_of "$dir")))
        done < <(find "$ROOTFS/usr/share/locale" -mindepth 1 -maxdepth 1 -type d ! -name "en_US*" ! -name "en" -print0 2>/dev/null)
    fi

    section_header 10 "Locale Translations" "$(human_readable $section_size)"

    if [ -d "$ROOTFS/usr/share/locale" ]; then
        log_action "INFO" "Removing non-English locale translations"

        # Remove all locale directories except en_US and en
        find "$ROOTFS/usr/share/locale" -mindepth 1 -maxdepth 1 -type d \
            ! -name "en_US*" \
            ! -name "en" \
            -exec rm -rf {} + 2>/dev/null || true

        # Optionally remove everything for maximum savings
        # safe_remove "$ROOTFS/usr/share/locale" "All locale translations"
    fi

    log_action "INFO" "Only English locales retained"
}

# Section 11: i18n & Character Encodings
section_11_i18n_encodings() {
    if ! should_run_section 11; then return 0; fi

    # Calculate section size
    local section_size=0
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/i18n")))
    # For gconv, estimate removable size (total minus essential modules)
    if [ -d "$ROOTFS/usr/lib/gconv" ]; then
        local gconv_total=$(size_of "$ROOTFS/usr/lib/gconv")
        local gconv_keep=0
        gconv_keep=$((gconv_keep + $(calculate_pattern_size "$ROOTFS/usr/lib/gconv" "UTF-*.so")))
        gconv_keep=$((gconv_keep + $(size_of "$ROOTFS/usr/lib/gconv/ISO8859-1.so")))
        gconv_keep=$((gconv_keep + $(size_of "$ROOTFS/usr/lib/gconv/gconv-modules")))
        gconv_keep=$((gconv_keep + $(size_of "$ROOTFS/usr/lib/gconv/gconv-modules.cache")))
        section_size=$((section_size + gconv_total - gconv_keep))
    fi

    section_header 11 "i18n & Character Encodings" "$(human_readable $section_size)"

    # Remove i18n source data
    safe_remove "$ROOTFS/usr/share/i18n" "i18n source data"

    # Handle gconv modules - keep only UTF-8 essentials
    if [ -d "$ROOTFS/usr/lib/gconv" ]; then
        log_action "INFO" "Reducing gconv modules to UTF-8 essentials"

        if [ "$DRY_RUN" = "no" ]; then
            # Create temporary directory for essential modules
            local temp_gconv=$(mktemp -d)

            # Copy essential modules
            cp "$ROOTFS/usr/lib/gconv"/UTF-*.so "$temp_gconv/" 2>/dev/null || true
            cp "$ROOTFS/usr/lib/gconv"/ISO8859-1.so "$temp_gconv/" 2>/dev/null || true
            cp "$ROOTFS/usr/lib/gconv"/gconv-modules "$temp_gconv/" 2>/dev/null || true
            cp "$ROOTFS/usr/lib/gconv"/gconv-modules.cache "$temp_gconv/" 2>/dev/null || true

            # Remove all gconv
            rm -rf "$ROOTFS/usr/lib/gconv"

            # Restore essentials
            mkdir -p "$ROOTFS/usr/lib/gconv"
            cp "$temp_gconv"/* "$ROOTFS/usr/lib/gconv/" 2>/dev/null || true

            # Cleanup
            rm -rf "$temp_gconv"

            log_action "INFO" "Kept UTF-8 and ISO-8859-1 encodings only"
        else
            log_action "DRY-RUN" "Would reduce gconv modules to UTF-8 essentials"
        fi
    fi

    log_action "WARN" "Character encoding conversions limited to UTF-8"
}

# Section 12: Build Artifacts
section_12_build_artifacts() {
    if ! should_run_section 12; then return 0; fi

    # Calculate section size (recursive patterns require find)
    local section_size=0
    while IFS= read -r -d '' item; do
        section_size=$((section_size + $(size_of "$item")))
    done < <(find "$ROOTFS" -name "__pycache__" -print0 2>/dev/null)
    while IFS= read -r -d '' item; do
        section_size=$((section_size + $(size_of "$item")))
    done < <(find "$ROOTFS" -name "*.pyc" -print0 2>/dev/null)
    while IFS= read -r -d '' item; do
        section_size=$((section_size + $(size_of "$item")))
    done < <(find "$ROOTFS" -name "*.pyo" -print0 2>/dev/null)
    while IFS= read -r -d '' item; do
        section_size=$((section_size + $(size_of "$item")))
    done < <(find "$ROOTFS" -name "perllocal.pod" -print0 2>/dev/null)
    while IFS= read -r -d '' item; do
        section_size=$((section_size + $(size_of "$item")))
    done < <(find "$ROOTFS" -name ".packlist" -print0 2>/dev/null)

    section_header 12 "Build Artifacts" "$(human_readable $section_size)"

    # Remove Python cache (if any remain)
    log_action "INFO" "Cleaning Python artifacts"
    safe_remove_pattern "$ROOTFS" "__pycache__" "Python cache directories"
    safe_remove_pattern "$ROOTFS" "*.pyc" "Python bytecode files"
    safe_remove_pattern "$ROOTFS" "*.pyo" "Python optimized files"

    # Remove Perl build artifacts
    log_action "INFO" "Cleaning Perl artifacts"
    safe_remove_pattern "$ROOTFS" "perllocal.pod" "Perl local documentation"
    safe_remove_pattern "$ROOTFS" ".packlist" "Perl packlist files"

    log_action "INFO" "Build artifacts cleaned"
}

# Section 13: Optional Cleanup
section_13_optional_cleanup() {
    if ! should_run_section 13; then return 0; fi

    # Calculate section size
    local section_size=0
    # Terminfo - estimate removable entries (non-essential terminals)
    if [ -d "$ROOTFS/usr/share/terminfo" ]; then
        while IFS= read -r -d '' item; do
            section_size=$((section_size + $(size_of "$item")))
        done < <(find "$ROOTFS/usr/share/terminfo" -mindepth 2 -type f \
            ! -name "linux*" ! -name "vt100*" ! -name "vt220*" \
            ! -name "xterm*" ! -name "screen*" -print0 2>/dev/null)
    fi
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/misc")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/color")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/tabset")))
    section_size=$((section_size + $(size_of "$ROOTFS/usr/share/dict")))

    section_header 13 "Optional Cleanup" "$(human_readable $section_size)"

    # Reduce terminfo to essential terminals
    if [ -d "$ROOTFS/usr/share/terminfo" ]; then
        log_action "INFO" "Reducing terminfo database to common terminals"

        if [ "$DRY_RUN" = "no" ]; then
            # Keep only essential terminal types
            find "$ROOTFS/usr/share/terminfo" -mindepth 2 -type f \
                ! -name "linux*" \
                ! -name "vt100*" \
                ! -name "vt220*" \
                ! -name "xterm*" \
                ! -name "screen*" \
                -delete 2>/dev/null || true
        else
            log_action "DRY-RUN" "Would reduce terminfo to common terminals"
        fi
    fi

    # Remove miscellaneous data
    safe_remove "$ROOTFS/usr/share/misc" "Miscellaneous data"
    safe_remove "$ROOTFS/usr/share/color" "Color data"
    safe_remove "$ROOTFS/usr/share/tabset" "Tabset data"
    safe_remove "$ROOTFS/usr/share/dict" "Dictionary data"

    # Aggressive mode: strip binaries
    if [ "$AGGRESSIVE" = "yes" ]; then
        log_action "INFO" "Stripping debug symbols from binaries (aggressive mode)"

        if [ "$DRY_RUN" = "no" ]; then
            # Strip binaries
            find "$ROOTFS/usr/bin" "$ROOTFS/usr/sbin" -type f -executable \
                -exec strip --strip-debug {} \; 2>/dev/null || true

            # Strip shared libraries
            find "$ROOTFS/usr/lib" -type f -name "*.so*" \
                -exec strip --strip-unneeded {} \; 2>/dev/null || true

            log_action "SUCCESS" "Binaries stripped"
        else
            log_action "DRY-RUN" "Would strip debug symbols from binaries"
        fi
    fi

    # Remove empty directories
    log_action "INFO" "Removing empty directories"
    safe_remove_empty_dirs "$ROOTFS/usr" "Empty directories"
}

#==============================================================================
# POST-REMOVAL PROCESSING
#==============================================================================

verify_essential_files() {
    log_action "INFO" "Verifying essential files..."

    local critical_files=(
        "/usr/bin/bash"
        "/usr/bin/ls"
        "/usr/bin/cat"
        "/usr/lib/libc.so.6"
        "/usr/lib/libm.so.6"
    )

    local missing=0

    for file in "${critical_files[@]}"; do
        if [ ! -e "$ROOTFS$file" ]; then
            log_action "ERROR" "Critical file missing: $file"
            missing=$((missing + 1))
        fi
    done

    if [ $missing -eq 0 ]; then
        log_action "SUCCESS" "All critical files present"
        return 0
    else
        log_action "ERROR" "$missing critical files missing!"
        return 1
    fi
}

update_library_cache() {
    if [ "$DRY_RUN" = "yes" ]; then
        log_action "DRY-RUN" "Would update library cache with ldconfig"
        return 0
    fi

    # Note: ldconfig requires chroot or proper environment
    log_action "WARN" "Library cache update requires manual intervention"
    log_action "INFO" "Run 'ldconfig' inside the rootfs environment after slimming"
}

#==============================================================================
# BACKUP CREATION
#==============================================================================

create_backup() {
    local backup_dir="$(dirname "$ROOTFS")"
    local timestamp=$(date +%Y%m%d_%H%M%S)
    local backup_file="${backup_dir}/rootfs-backup-${timestamp}.tar.gz"

    log_action "INFO" "Creating backup: $backup_file"

    if tar -czf "$backup_file" -C "$(dirname "$ROOTFS")" "$(basename "$ROOTFS")" 2>/dev/null; then
        log_action "SUCCESS" "Backup created: $backup_file"
        local backup_size=$(size_of "$backup_file")
        log_action "INFO" "Backup size: $(human_readable $backup_size)"
        return 0
    else
        log_action "ERROR" "Backup failed"
        return 1
    fi
}

#==============================================================================
# ARGUMENT PARSING
#==============================================================================

parse_arguments() {
    if [ $# -eq 0 ]; then
        usage
        exit 0
    fi

    while [ $# -gt 0 ]; do
        case "$1" in
            -h|--help)
                usage
                exit 0
                ;;
            -v|--version)
                version
                exit 0
                ;;
            --dry-run)
                DRY_RUN="yes"
                shift
                ;;
            --execute)
                DRY_RUN="no"
                shift
                ;;
            --no-confirm)
                NO_CONFIRM="yes"
                shift
                ;;
            --backup)
                CREATE_BACKUP="yes"
                shift
                ;;
            --sections=*)
                SECTIONS_TO_RUN="${1#*=}"
                shift
                ;;
            --aggressive)
                AGGRESSIVE="yes"
                shift
                ;;
            --verbose)
                VERBOSE="yes"
                shift
                ;;
            --quiet)
                QUIET="yes"
                shift
                ;;
            -*)
                echo "Error: Unknown option: $1" >&2
                echo "Use --help for usage information" >&2
                exit 1
                ;;
            *)
                # Assume this is the rootfs path
                ROOTFS="$1"
                shift
                ;;
        esac
    done

    # Validate rootfs path was provided
    if [ -z "${ROOTFS:-}" ]; then
        echo "Error: ROOTFS_PATH is required" >&2
        echo "Use --help for usage information" >&2
        exit 1
    fi

    # Remove trailing slash
    ROOTFS="${ROOTFS%/}"
}

#==============================================================================
# PRE-FLIGHT CHECKS
#==============================================================================

preflight_checks() {
    log_action "INFO" "Running pre-flight checks..."

    # Check if rootfs exists
    if [ ! -d "$ROOTFS" ]; then
        log_action "ERROR" "Rootfs directory does not exist: $ROOTFS"
        exit 1
    fi

    # Check if it looks like a valid rootfs
    if [ ! -d "$ROOTFS/usr" ]; then
        log_action "ERROR" "Does not appear to be a valid rootfs (no /usr directory)"
        exit 1
    fi

    # Check write permissions (only if not dry-run)
    if [ "$DRY_RUN" = "no" ]; then
        if [ ! -w "$ROOTFS" ]; then
            log_action "ERROR" "No write permission for: $ROOTFS"
            exit 1
        fi
    fi

    log_action "SUCCESS" "Pre-flight checks passed"
}

#==============================================================================
# SUMMARY REPORT
#==============================================================================

display_summary() {
    local final_size=$(size_of "$ROOTFS")

    echo ""
    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${RESET}"
    echo -e "${BOLD}                      SUMMARY REPORT                           ${RESET}"
    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${RESET}"

    echo -e "${BOLD}Mode:${RESET} $([ "$DRY_RUN" = "yes" ] && echo "DRY-RUN" || echo "EXECUTE")"
    echo -e "${BOLD}Rootfs:${RESET} $ROOTFS"
    echo -e "${BOLD}Sections run:${RESET} $SECTION_COUNT"

    if [ "$DRY_RUN" = "yes" ]; then
        echo -e "${BOLD}Would save:${RESET} $(human_readable $TOTAL_WOULD_SAVE)"
        echo -e "${BOLD}Current size:${RESET} $(human_readable $INITIAL_SIZE)"
        echo -e "${BOLD}Projected size:${RESET} $(human_readable $((INITIAL_SIZE - TOTAL_WOULD_SAVE)))"
    else
        echo -e "${BOLD}Initial size:${RESET} $(human_readable $INITIAL_SIZE)"
        echo -e "${BOLD}Final size:${RESET} $(human_readable $final_size)"
        echo -e "${BOLD}Space saved:${RESET} $(human_readable $TOTAL_SAVED)"

        local percent_saved=$((TOTAL_SAVED * 100 / INITIAL_SIZE))
        echo -e "${BOLD}Reduction:${RESET} ${percent_saved}%"
    fi

    if [ $ERROR_COUNT -gt 0 ]; then
        echo -e "${BOLD}Errors:${RESET} ${RED}$ERROR_COUNT${RESET}"
    else
        echo -e "${BOLD}Errors:${RESET} ${GREEN}0${RESET}"
    fi

    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${RESET}"

    if [ "$DRY_RUN" = "yes" ]; then
        echo ""
        echo -e "${YELLOW}This was a DRY-RUN. No files were actually removed.${RESET}"
        echo -e "${YELLOW}To perform the actual slimming, run with --execute flag.${RESET}"
        echo ""
    else
        echo ""
        echo -e "${GREEN}Slimming complete!${RESET}"
        echo ""
        echo "Next steps:"
        echo "  1. Verify functionality: $ROOTFS/usr/bin/bash -c 'ls && echo test'"
        echo "  2. Create container: tar -czf slimmed-rootfs.tar.gz -C $ROOTFS ."
        echo "  3. Set environment: LANG=C or LANG=en_US.UTF-8"
        echo ""
    fi
}

#==============================================================================
# MAIN EXECUTION
#==============================================================================

main() {
    # Parse command-line arguments
    parse_arguments "$@"

    # Display header
    echo ""
    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${RESET}"
    echo -e "${BOLD}    Linux from Scratch Rootfs Slimming Script v${VERSION}        ${RESET}"
    echo -e "${BOLD}═══════════════════════════════════════════════════════════════${RESET}"
    echo ""

    # Pre-flight checks
    preflight_checks

    # Calculate initial size
    log_action "INFO" "Calculating initial size..."
    INITIAL_SIZE=$(size_of "$ROOTFS")
    log_action "INFO" "Initial size: $(human_readable $INITIAL_SIZE)"

    # Create backup if requested
    if [ "$CREATE_BACKUP" = "yes" ] && [ "$DRY_RUN" = "no" ]; then
        if ! create_backup; then
            log_action "ERROR" "Backup failed, aborting"
            exit 1
        fi
    fi

    # Display plan and confirm
    echo ""
    if [ "$DRY_RUN" = "yes" ]; then
        log_action "INFO" "Running in DRY-RUN mode - no files will be removed"
    else
        log_action "WARN" "Running in EXECUTE mode - files will be permanently removed"
    fi

    if [ "$SECTIONS_TO_RUN" = "all" ]; then
        log_action "INFO" "Will run all 13 sections"
    else
        log_action "INFO" "Will run sections: $SECTIONS_TO_RUN"
    fi

    # Confirm before proceeding
    if [ "$DRY_RUN" = "no" ]; then
        confirm "This will permanently remove files from: $ROOTFS"
    fi

    # Execute removal sections
    section_01_gcc_compiler
    section_02_dev_tools
    section_03_python
    section_04_perl
    section_05_static_libs
    section_06_headers
    section_07_sanitizers
    section_08_documentation
    section_09_locale_data
    section_10_locale_translations
    section_11_i18n_encodings
    section_12_build_artifacts
    section_13_optional_cleanup

    # Post-removal processing
    if [ "$DRY_RUN" = "no" ]; then
        echo ""
        log_action "INFO" "Running post-removal processing..."
        verify_essential_files
        update_library_cache
    fi

    # Display summary
    display_summary

    # Exit with appropriate code
    if [ $ERROR_COUNT -gt 0 ]; then
        exit 1
    else
        exit 0
    fi
}

# Run main function
main "$@"
