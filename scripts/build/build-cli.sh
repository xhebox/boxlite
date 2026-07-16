#!/bin/bash
# Build the BoxLite CLI.
#
# Usage:
#   ./build-cli.sh
#
# BUILD_PROFILE selects debug or release; the default is release.

set -e

SCRIPT_BUILD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_BUILD_DIR/common.sh"

PROFILE="${BUILD_PROFILE:-release}"
set_cargo_profile "$PROFILE"

build_cli() {
    local cargo_args=(build -p boxlite-cli)

    if [ -n "$CARGO_PROFILE_ARG" ]; then
        cargo_args+=("$CARGO_PROFILE_ARG")
    fi

    cd "$PROJECT_ROOT"
    "${CARGO:-cargo}" "${cargo_args[@]}"
}

main() {
    print_header "Building BoxLite CLI ($PROFILE)"
    build_cli
    print_success "BoxLite CLI built at $PROJECT_ROOT/target/$PROFILE/boxlite"
}

main "$@"
