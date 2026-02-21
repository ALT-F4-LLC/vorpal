#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# End-to-end integration tests for TypeScript SDK support
#
# This script validates aspects of the TypeScript integration that require
# the actual CLI binary or running Vorpal services. Template validation,
# syntax checking, SDK exports, and digest parity are covered by the Bun
# test suite in sdk/typescript/src/__tests__/ and should NOT be duplicated
# here.
#
# What this script covers:
#   1. TypeScript SDK unit tests (delegates to bun test)
#   2. Cross-SDK parity: Rust, Go, and TypeScript produce identical digests
#
# Usage:
#   ./tests/typescript-integration.sh           # Run all tests
#   ./tests/typescript-integration.sh --quick   # Run only offline tests (no vorpal services)
#
# Prerequisites for full run:
#   - Vorpal services running (make vorpal-start)
#   - Cargo built (make build)
#   - Vorpal.toml, Vorpal.go.toml, Vorpal.ts.toml exist in project root
#
# Exit codes:
#   0 = all tests passed
#   1 = one or more tests failed
# ---------------------------------------------------------------------------

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

PASS=0
FAIL=0
SKIP=0
QUICK_MODE=false

if [[ "${1:-}" == "--quick" ]]; then
    QUICK_MODE=true
fi

# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

pass() {
    PASS=$((PASS + 1))
    echo "  PASS: $1"
}

fail() {
    FAIL=$((FAIL + 1))
    echo "  FAIL: $1"
    if [[ -n "${2:-}" ]]; then
        echo "        $2"
    fi
}

skip() {
    SKIP=$((SKIP + 1))
    echo "  SKIP: $1"
}

section() {
    echo ""
    echo "=== $1 ==="
}

# ---------------------------------------------------------------------------
# Test 1: TypeScript SDK unit tests (bun test)
#
# Run the TypeScript SDK's own unit tests to verify template validation,
# syntax checking, SDK exports, and digest parity.
# ---------------------------------------------------------------------------

section "TypeScript SDK Unit Tests"

TS_SDK_DIR="${PROJECT_ROOT}/sdk/typescript"

if command -v bun >/dev/null 2>&1; then
    if (cd "${TS_SDK_DIR}" && bun test 2>&1); then
        pass "TypeScript SDK unit tests pass"
    else
        fail "TypeScript SDK unit tests failed"
    fi
else
    skip "bun not found; skipping TypeScript SDK unit tests"
fi

# ---------------------------------------------------------------------------
# Test 2: Cross-SDK Parity (requires vorpal services)
#
# This test follows the same pattern as the CI parity tests in
# .github/workflows/vorpal.yaml. It builds the same artifact with
# Rust, Go, and TypeScript configs and compares the SHA-256 digests.
#
# Prerequisites:
#   - vorpal services running
#   - cargo built
#   - Vorpal.ts.toml exists (or a TypeScript config for parity testing)
#
# TODO: Create Vorpal.ts.toml in the project root that defines the same
# artifacts as Vorpal.toml (Rust) and Vorpal.go.toml (Go) to enable
# three-way cross-SDK digest comparison. Once created, uncomment the
# parity test below.
# ---------------------------------------------------------------------------

section "Cross-SDK Parity Tests"

if [[ "${QUICK_MODE}" == "true" ]]; then
    skip "Cross-SDK parity tests (--quick mode)"
else
    VORPAL_BIN="${PROJECT_ROOT}/target/debug/vorpal"

    if [[ ! -x "${VORPAL_BIN}" ]]; then
        skip "vorpal binary not found at ${VORPAL_BIN}; run 'make build' first"
    elif ! "${VORPAL_BIN}" --help >/dev/null 2>&1; then
        skip "vorpal binary not functional"
    else
        RUST_CONFIG="${PROJECT_ROOT}/Vorpal.toml"
        GO_CONFIG="${PROJECT_ROOT}/Vorpal.go.toml"
        TS_CONFIG="${PROJECT_ROOT}/Vorpal.ts.toml"

        if [[ ! -f "${RUST_CONFIG}" ]]; then
            skip "Vorpal.toml (Rust config) not found"
        elif [[ ! -f "${GO_CONFIG}" ]]; then
            skip "Vorpal.go.toml (Go config) not found"
        elif [[ ! -f "${TS_CONFIG}" ]]; then
            skip "Vorpal.ts.toml (TypeScript config) not found — create it to enable parity tests"
        else
            RUST_DIGEST=$("${VORPAL_BIN}" build "vorpal" 2>&1 | tail -1)
            GO_DIGEST=$("${VORPAL_BIN}" build --config "${GO_CONFIG}" "vorpal" 2>&1 | tail -1)
            TS_DIGEST=$("${VORPAL_BIN}" build --config "${TS_CONFIG}" "vorpal" 2>&1 | tail -1)

            if [[ "${RUST_DIGEST}" == "${GO_DIGEST}" ]] && [[ "${GO_DIGEST}" == "${TS_DIGEST}" ]]; then
                pass "Cross-SDK parity: Rust, Go, and TypeScript produce identical digests"
                echo "        Digest: ${RUST_DIGEST}"
            else
                fail "Cross-SDK parity: digests differ" \
                    "Rust=${RUST_DIGEST} Go=${GO_DIGEST} TypeScript=${TS_DIGEST}"
            fi
        fi
    fi
fi

# ---------------------------------------------------------------------------
# Summary
# ---------------------------------------------------------------------------

echo ""
echo "==========================================="
echo "  Results: ${PASS} passed, ${FAIL} failed, ${SKIP} skipped"
echo "==========================================="

if [[ ${FAIL} -gt 0 ]]; then
    exit 1
fi

exit 0
