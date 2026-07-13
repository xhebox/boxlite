#!/bin/bash
# Build boxlite-guest binary on macOS or Linux
#
# Prerequisites: Run the appropriate setup script first:
#   - macOS: scripts/setup/setup-macos.sh
#   - Ubuntu/Debian: scripts/setup/setup-ubuntu.sh
#   - musllinux: scripts/setup/setup-musllinux.sh
#
# Usage:
#   ./build-guest.sh [--dest-dir DIR] [--profile PROFILE]
#
# Options:
#   --dest-dir DIR      Directory to copy the guest binary to
#   --profile PROFILE   Build profile: release or debug (default: release)

set -e

# Load shared build-context and setup utilities
SCRIPT_BUILD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./common.sh
source "$SCRIPT_BUILD_DIR/common.sh"
source "$SCRIPT_DIR/setup/setup-common.sh"

# Capture original working directory before any cd commands
ORIG_DIR="$(pwd)"

# Parse command-line arguments
parse_args() {
    DEST_DIR_ARG=""
    PROFILE="release"

    while [[ $# -gt 0 ]]; do
        case $1 in
            --dest-dir)
                DEST_DIR_ARG="$2"
                shift 2
                ;;
            --profile)
                PROFILE="$2"
                shift 2
                ;;
            *)
                echo "Unknown option: $1"
                echo "Usage: $0 [--dest-dir DIR] [--profile PROFILE]"
                exit 1
                ;;
        esac
    done

    set_cargo_profile "$PROFILE" || exit 1
    DEST_DIR=$(resolve_path_from "$ORIG_DIR" "$DEST_DIR_ARG")
}

parse_args "$@"
CARGO_TARGET_ROOT=$(cargo_target_dir)
# shellcheck source=../util.sh
source "$SCRIPT_DIR/util.sh"
GUEST_BINARY_PATH="$CARGO_TARGET_ROOT/$GUEST_TARGET/$PROFILE/boxlite-guest"

RUSTC_HOST=$(rustc_host_triple)

# Detect OS
OS=$(detect_os)
print_header "Building boxlite-guest on $OS..."

# Verify prerequisites (fail fast)
check_prerequisites() {
    print_section "Checking prerequisites..."
    require_command "rustc" "Run: scripts/setup/setup-macos.sh (or setup-ubuntu.sh)"

    if [[ "$RUSTC_HOST" == *-musl ]]; then
        print_info "Using native musl Rust toolchain: $RUSTC_HOST"
    else
        require_musl
    fi

    print_success "All prerequisites satisfied"
    echo ""
}

# Ensure Rust target is added
setup_rust_target() {
    print_step "Checking Rust target $GUEST_TARGET... "

    if [ "$RUSTC_HOST" = "$GUEST_TARGET" ]; then
        print_success "Using native host target"
    elif command -v rustup >/dev/null 2>&1; then
        if rustup target list | grep -q "$GUEST_TARGET (installed)"; then
            print_success "Already installed"
        else
            echo -e "${YELLOW}Adding...${NC}"
            rustup target add "$GUEST_TARGET"
            print_success "Target added"
        fi
    else
        print_error "rustup not found and rustc host is $RUSTC_HOST, not $GUEST_TARGET"
        exit 1
    fi
}

# Verify the guest has no dynamic interpreter or dependencies.
verify_guest_binary() {
    local file_output
    file_output=$(file "$GUEST_BINARY_PATH")
    if echo "$file_output" | grep -q "dynamically linked"; then
        if command -v readelf >/dev/null 2>&1 \
            && ! readelf -d "$GUEST_BINARY_PATH" | grep -q "(NEEDED)" \
            && ! readelf -l "$GUEST_BINARY_PATH" | grep -q "INTERP"; then
            print_info "boxlite-guest is static PIE (no dynamic dependencies or interpreter)"
            return
        fi

        local musl_arch
        musl_arch=$(echo "$GUEST_TARGET" | cut -d'-' -f1)
        local musl_gcc="${musl_arch}-linux-musl-gcc"
        print_error "boxlite-guest is dynamically linked, but must be statically linked"
        echo "The guest binary at $GUEST_BINARY_PATH depends on libraries unavailable inside the VM."
        echo "Check that $musl_gcc is a musl compiler: $musl_gcc --version"
        exit 1
    fi
}

# Build the guest binary
build_guest_binary() {
    cd "$PROJECT_ROOT"
    echo "🔨 Building guest binary for $GUEST_TARGET $PROFILE..."

    # macOS cross-compilation needs musl-cross linker.
    # The project .cargo/config.toml is platform-agnostic (no linker).
    # Set the linker via env var as fallback if ~/.cargo/config.toml isn't configured.
    if [ "$OS" = "macos" ]; then
        local arch_prefix
        arch_prefix=$(echo "$GUEST_TARGET" | cut -d'-' -f1)
        local env_var_name
        env_var_name="CARGO_TARGET_$(echo "$GUEST_TARGET" | tr '[:lower:]-' '[:upper:]_')_LINKER"
        if [ -z "${!env_var_name:-}" ]; then
            export "$env_var_name=${arch_prefix}-linux-musl-gcc"
        fi
    fi

    # libseccomp is enabled in src/guest/Cargo.toml ("libseccomp" feature on
    # libcontainer). The Rust libseccomp-sys crate needs libseccomp.a built for
    # the *target* triple. Build/cache it and export the env vars libseccomp-sys
    # reads in its build.rs.
    # shellcheck source=./build-libseccomp.sh
    source "$SCRIPT_BUILD_DIR/build-libseccomp.sh"
    ensure_libseccomp_for_target "$GUEST_TARGET"

    local cargo_args
    cargo_args=(build)
    if [ -n "$CARGO_PROFILE_ARG" ]; then
        cargo_args+=("$CARGO_PROFILE_ARG")
    fi
    cargo_args+=(--target "$GUEST_TARGET" -p boxlite-guest)
    (cd "$PROJECT_ROOT/src/guest" && CARGO_TARGET_DIR="$CARGO_TARGET_ROOT" cargo "${cargo_args[@]}")

    verify_guest_binary
}

# Copy binary to destination
copy_to_destination() {
    if [ -z "$DEST_DIR" ]; then
        echo "✅ Guest binary built successfully (no destination specified)"
        echo "Binary location: $GUEST_BINARY_PATH"
        return 0
    fi

    # Relative paths are relative to caller's working directory (already correct behavior)
    # Absolute paths are used as-is
    echo "📦 Copying to destination: $DEST_DIR"
    mkdir -p "$DEST_DIR"
    cp "$GUEST_BINARY_PATH" "$DEST_DIR/"

    echo "✅ Guest binary built and copied to $DEST_DIR"
    echo "Binary info:"
    ls -lh "$DEST_DIR/boxlite-guest"
    file "$DEST_DIR/boxlite-guest"
}

# Main execution
main() {
    if [ "${SKIP_GUEST_BUILD:-0}" = "1" ]; then
        if [ ! -x "$GUEST_BINARY_PATH" ]; then
            print_error "SKIP_GUEST_BUILD=1 but guest binary not found at $GUEST_BINARY_PATH"
            exit 1
        fi
        print_success "Using pre-built guest: $GUEST_BINARY_PATH"
    else
        check_prerequisites
        setup_rust_target
        build_guest_binary
    fi
    copy_to_destination

    echo ""
    print_success "Done! Guest binary is ready for packaging."
}

main "$@"
