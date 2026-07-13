#!/bin/bash
# Build the BoxLite CLI.

set -e

SCRIPT_BUILD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_BUILD_DIR/common.sh"

PROFILE="${BUILD_PROFILE:-release}"

set_cargo_profile "$PROFILE"

cargo_args=(build -p boxlite-cli)
if [ -n "$CARGO_PROFILE_ARG" ]; then
    cargo_args+=("$CARGO_PROFILE_ARG")
fi

cd "$PROJECT_ROOT"
"${CARGO:-cargo}" "${cargo_args[@]}"
