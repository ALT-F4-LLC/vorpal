#!/usr/bin/env bash
set -euo pipefail

REPO_PATH="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
VERIFY_LIB="${REPO_PATH}/script/dev/lib/verify.sh"

# SHA-256 of the exact byte string written by write_payload, computed
# independently of the helper (published test vector for "abc").
PAYLOAD_DIGEST="ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"

FAILURES=0
WORK_DIR="$(mktemp -d)"

trap 'rm -rf "${WORK_DIR}"' EXIT

write_payload() {
    printf 'abc' >"${1}"
}

fail() {
    echo "FAIL: ${1}" >&2
    FAILURES=$((FAILURES + 1))
}

pass() {
    echo "ok: ${1}"
}

run_verify() {
    (
        set +e
        # shellcheck source=../dev/lib/verify.sh
        . "${VERIFY_LIB}"
        verify_sha256 "${1}" "${2}" 2>"${WORK_DIR}/stderr"
        echo "${?}" >"${WORK_DIR}/status"
    )
}

matching_archive_is_accepted() {
    write_payload "${WORK_DIR}/archive"
    run_verify "${WORK_DIR}/archive" "${PAYLOAD_DIGEST}"

    if [[ "$(cat "${WORK_DIR}/status")" != "0" ]]; then
        fail "untampered archive was refused: $(cat "${WORK_DIR}/stderr")"
        return
    fi

    pass "untampered archive is accepted"
}

tampered_archive_is_refused_naming_both_digests() {
    write_payload "${WORK_DIR}/archive"
    printf 'x' >>"${WORK_DIR}/archive"
    run_verify "${WORK_DIR}/archive" "${PAYLOAD_DIGEST}"

    if [[ "$(cat "${WORK_DIR}/status")" == "0" ]]; then
        fail "tampered archive was accepted"
        return
    fi

    if ! /usr/bin/grep -q "${PAYLOAD_DIGEST}" "${WORK_DIR}/stderr"; then
        fail "refusal did not name the expected digest: $(cat "${WORK_DIR}/stderr")"
        return
    fi

    # The digest of "abcx", from `openssl dgst -sha256`, not from the helper.
    if ! /usr/bin/grep -q "7571ce1f8e21c6b13dd7ec2c5ec7c9e4dd9852e209869511853f2f1f74b17927" "${WORK_DIR}/stderr"; then
        fail "refusal did not name the actual digest: $(cat "${WORK_DIR}/stderr")"
        return
    fi

    pass "tampered archive is refused naming both digests"
}

missing_pin_is_refused() {
    write_payload "${WORK_DIR}/archive"
    run_verify "${WORK_DIR}/archive" ""

    if [[ "$(cat "${WORK_DIR}/status")" == "0" ]]; then
        fail "empty pin was accepted"
        return
    fi

    pass "empty pin is refused"
}

missing_hasher_is_refused() {
    write_payload "${WORK_DIR}/archive"

    mkdir -p "${WORK_DIR}/emptybin"

    (
        set +e
        export PATH="${WORK_DIR}/emptybin"
        # shellcheck source=../dev/lib/verify.sh
        . "${VERIFY_LIB}"
        verify_sha256 "${WORK_DIR}/archive" "${PAYLOAD_DIGEST}" 2>"${WORK_DIR}/stderr"
        echo "${?}" >"${WORK_DIR}/status"
    )

    if [[ "$(cat "${WORK_DIR}/status")" == "0" ]]; then
        fail "verification succeeded with no hashing tool available"
        return
    fi

    pass "absent hashing tool is refused"
}

matching_archive_is_accepted
tampered_archive_is_refused_naming_both_digests
missing_pin_is_refused
missing_hasher_is_refused

if [[ "${FAILURES}" -ne 0 ]]; then
    echo "${FAILURES} check(s) failed" >&2
    exit 1
fi

echo "all checks passed"
