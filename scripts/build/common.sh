#!/bin/bash
# Shared build context for BoxLite build scripts.
# This file should be sourced by other scripts, not executed directly.

if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    echo "Error: This script should be sourced, not executed directly" >&2
    echo "Usage: source scripts/build/common.sh" >&2
    exit 1
fi

export SCRIPT_BUILD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_DIR="$(cd "$SCRIPT_BUILD_DIR/.." && pwd)"
# shellcheck source=../common.sh
source "$SCRIPT_DIR/common.sh"
export PROJECT_ROOT="$(cd "$SCRIPT_BUILD_DIR/../.." && pwd)"

rustc_host_triple() {
    local rustc_output
    local line
    local host

    if ! rustc_output=$(rustc -vV 2>&1); then
        echo "Error: rustc -vV failed; install Rust or ensure rustc is available in PATH" >&2
        return 1
    fi

    host=""
    while IFS= read -r line; do
        case "$line" in
            host:*)
                host=${line#host:}
                host=${host#"${host%%[![:space:]]*}"}
                if [ -n "$host" ]; then
                    printf '%s\n' "$host"
                    return 0
                fi
                ;;
        esac
    done <<< "$rustc_output"

    echo "Error: rustc -vV did not report a nonempty host triple" >&2
    return 1
}

set_cargo_profile() {
    case "${1:-}" in
        debug) CARGO_PROFILE_ARG="" ;;
        release) CARGO_PROFILE_ARG=--release ;;
        *)
            echo "Invalid profile: ${1:-}; use debug or release" >&2
            return 1
            ;;
    esac
}

resolve_path_from() {
    local base=$1
    local path=${2:-}

    case "$path" in
        "")
            printf '%s\n' ""
            ;;
        /*)
            printf '%s\n' "$path"
            ;;
        *)
            printf '%s\n' "$base/$path"
            ;;
    esac
}

cargo_target_dir() {
    local target_dir=${CARGO_TARGET_DIR:-}

    case "$target_dir" in
        "")
            printf '%s\n' "$PROJECT_ROOT/target"
            ;;
        /*)
            printf '%s\n' "$target_dir"
            ;;
        *)
            printf '%s\n' "$PROJECT_ROOT/$target_dir"
            ;;
    esac
}
