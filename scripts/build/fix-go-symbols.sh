#!/bin/bash
# Prepare libboxlite.a for linking into a Go binary.
#
# libgvproxy (a Go c-archive) is statically linked into libboxlite.a,
# bringing Go runtime symbols that conflict with the Go SDK binary's own
# runtime. This script localizes those symbols so the Go binary's runtime
# takes precedence.
#
# Rust staticlibs intentionally leave native runtime libraries unbundled. On
# musl that includes libunwind, which cgo cannot discover from rustc's
# native-static-libs metadata. If the archive needs an unwinder and does not
# already provide one, merge the toolchain's static libunwind into this
# Go-specific archive before cgo sees it.
#
# Requires: llvm-objcopy (LLVM 20+ on macOS, LLVM 9+ on Linux), ar, nm
#
# Usage:
#   ./fix-go-symbols.sh <path/to/libboxlite.a>

set -e

# Load common utilities
SCRIPT_BUILD_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCRIPT_DIR="$(cd "$SCRIPT_BUILD_DIR/.." && pwd)"
source "$SCRIPT_DIR/common.sh"

LIB="${1:?Usage: fix-go-symbols.sh <path/to/libboxlite.a>}"

if [ ! -f "$LIB" ]; then
    print_error "Library not found: $LIB"
    exit 1
fi

# Resolve llvm-objcopy per platform.
OS=$(detect_os)
case "$OS" in
    macos)
        OBJCOPY="${LLVM_OBJCOPY:-$(/opt/homebrew/bin/brew --prefix llvm 2>/dev/null || echo /opt/homebrew/opt/llvm)/bin/llvm-objcopy}"
        ;;
    linux)
        OBJCOPY="${LLVM_OBJCOPY:-llvm-objcopy}"
        AR="${AR:-ar}"
        NM="${NM:-nm}"
        ;;
    *)
        print_error "Unsupported platform: $(uname -s)"
        exit 1
        ;;
esac

require_command "$OBJCOPY" "Install LLVM (brew install llvm on macOS)"

bundle_unwinder() {
    require_command "$NM" "Install binutils"
    require_command "$AR" "Install binutils"

    if ! "$NM" -u "$LIB" | awk '{ print $NF }' | grep -q '^_Unwind_'; then
        return
    fi

    if "$NM" -g --defined-only "$LIB" | awk '{ print $NF }' | grep -q '^_Unwind_RaiseException$'; then
        return
    fi

    local unwind_lib temp_dir merged
    unwind_lib="$(rustc --print target-libdir)/self-contained/libunwind.a"

    if [ ! -f "$unwind_lib" ]; then
        unwind_lib="$("${CC:-cc}" --print-file-name=libunwind.a)"
    fi

    if [ ! -f "$unwind_lib" ]; then
        return
    fi
    temp_dir="$(mktemp -d)"
    merged="$temp_dir/libboxlite.a"
    trap 'rm -rf "$temp_dir"' RETURN

    "$AR" -M <<EOF
CREATE $merged
ADDLIB $LIB
ADDLIB $unwind_lib
SAVE
END
EOF

    chmod --reference="$LIB" "$merged"
    mv "$merged" "$LIB"
    print_success "Static unwinder bundled from $unwind_lib"
}

# CGo bridge symbols from embedded libgvproxy conflict with the Go SDK
# binary's own runtime. Localizing them lets the binary's runtime win.
#
# We use --wildcard with [a-z] character classes to match Go runtime symbols
# (_cgo_panic, x_cgo_init, crosscall2, etc.) while preserving package-specific
# CGo function bridges (_cgo_<hash>_Cfunc_*) which start with a hex digit.
#
# On Linux ELF, the embedded Go c-archive also has .init_array constructors
# that try to start a second Go runtime, causing a segfault. Removing the
# section prevents double-init while the main binary's runtime handles
# everything (same Go version, same ABI).
case "$OS" in
    linux)
        "$OBJCOPY" \
            --remove-section .init_array \
            --wildcard \
            --localize-symbol='_cgo_[a-z]*' \
            --localize-symbol='x_cgo_*' \
            --localize-symbol='crosscall*' \
            "$LIB"
        bundle_unwinder
        ;;
    macos)
        "$OBJCOPY" \
            --wildcard \
            --localize-symbol='__cgo_[a-z]*' \
            --localize-symbol='_x_cgo_*' \
            --localize-symbol='_crosscall*' \
            "$LIB"
        ;;
esac

print_success "Go symbols fixed in $(basename "$LIB")"
