#!/bin/bash
# Build the BoxLite CLI for a matching runtime profile.

set -e

SCRIPT_BUILD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_BUILD_DIR/common.sh"

PROFILE=release

while [ "$#" -gt 0 ]; do
    case "$1" in
        --profile)
            if [ -z "${2:-}" ]; then
                echo "--profile requires debug or release" >&2
                exit 2
            fi
            PROFILE=$2
            shift 2
            ;;
        --help|-h)
            echo "Usage: $0 [--profile debug|release]"
            exit 0
            ;;
        *)
            echo "Unknown option: $1" >&2
            echo "Usage: $0 [--profile debug|release]" >&2
            exit 2
            ;;
    esac
done

set_cargo_profile "$PROFILE"

cargo_args=(build -p boxlite-cli)
if [ -n "$CARGO_PROFILE_ARG" ]; then
    cargo_args+=("$CARGO_PROFILE_ARG")
fi

cd "$PROJECT_ROOT"
"${CARGO:-cargo}" "${cargo_args[@]}"
