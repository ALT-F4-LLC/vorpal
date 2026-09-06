#!/usr/bin/env bash

verify_sha256() {
    local archive="${1}"
    local expected="${2}"
    local actual=""

    if [[ "${expected}" == "" ]]; then
        echo "No pinned SHA-256 for ${archive} on this tool, OS and architecture" >&2
        return 1
    fi

    if [[ ! -f "${archive}" ]]; then
        echo "Cannot verify ${archive}: not a regular file" >&2
        return 1
    fi

    if command -v sha256sum >/dev/null 2>&1; then
        actual="$(sha256sum "${archive}" | cut -d' ' -f1)"
    elif command -v shasum >/dev/null 2>&1; then
        actual="$(shasum -a 256 "${archive}" | cut -d' ' -f1)"
    else
        echo "Cannot verify ${archive}: neither sha256sum nor shasum is available" >&2
        return 1
    fi

    if [[ "${actual}" != "${expected}" ]]; then
        echo "Checksum mismatch for ${archive}" >&2
        echo "  expected sha256: ${expected}" >&2
        echo "  actual sha256:   ${actual}" >&2
        return 1
    fi
}
