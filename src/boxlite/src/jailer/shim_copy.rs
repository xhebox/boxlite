//! Shim binary and libkrunfw copy utility (Firecracker pattern).
//!
//! This module implements Firecracker's security isolation pattern:
//! copy (not hard-link) the shim binary into the jail directory to ensure
//! complete memory isolation between boxes.
//!
//! Any packaged `libkrunfw.*` artifacts are copied beside the shim:
//! `libkrunfw.bin` supports the explicit `krun_set_kernel` path, while
//! `libkrunfw.so*` supports libkrun's `dlopen` fallback.
//!
//! # Why Copy Instead of Hard-Link?
//!
//! 1. **Memory Isolation**: Hard-linked binaries share the same inode,
//!    which means they share the same `.text` section in memory.
//!    A vulnerability in one box could potentially exploit shared code.
//!
//! 2. **Independent Updates**: Each box has its own copy, so updates
//!    to the shim don't affect running boxes.
//!
//! # Usage
//!
//! ```ignore
//! use boxlite::jailer::shim_copy::copy_shim_to_box;
//!
//! let copied_shim = copy_shim_to_box(&shim_path, &box_dir)?;
//! // copied_shim is now at box_dir/bin/boxlite-shim
//! ```

use crate::jailer::common::fs::copy_if_newer;
use boxlite_shared::errors::{BoxliteError, BoxliteResult};
use std::path::{Path, PathBuf};

/// Library file name prefixes to copy alongside the shim binary.
///
/// The prefix covers both the optional raw kernel (`libkrunfw.bin`) and the
/// shared library (`libkrunfw.so*`).
#[cfg(any(target_os = "linux", target_os = "macos"))]
const LIBKRUNFW_PREFIX: &str = "libkrunfw.";

/// Copy shim binary and libkrunfw to box directory for jail isolation.
///
/// This follows Firecracker's approach: copy (not hard-link) the shim binary
/// into the jail directory to ensure complete memory isolation between boxes.
/// Any packaged `libkrunfw.*` artifacts are copied with it.
///
/// # Arguments
///
/// * `shim_path` - Path to the original shim binary
/// * `box_dir` - Path to the box directory (e.g., `~/.boxlite/boxes/{box_id}`)
///
/// # Returns
///
/// Path to the copied shim binary (inside `box_dir/bin/`).
///
/// # Errors
///
/// Returns [`BoxliteError::Storage`] if:
/// - Failed to create the `bin/` directory
/// - Failed to copy the shim binary
/// - Failed to copy libkrunfw
///
/// # Example
///
/// ```ignore
/// let copied_shim = copy_shim_to_box(&shim_path, &box_dir)?;
/// // Use copied_shim instead of original shim_path
/// ```
pub fn copy_shim_to_box(shim_path: &Path, box_dir: &Path) -> BoxliteResult<PathBuf> {
    let bin_dir = box_dir.join("bin");
    std::fs::create_dir_all(&bin_dir).map_err(|e| {
        BoxliteError::Storage(format!(
            "Failed to create bin directory {}: {}",
            bin_dir.display(),
            e
        ))
    })?;

    // Copy shim binary
    let shim_name = shim_path.file_name().unwrap_or_default();
    let dest_shim = bin_dir.join(shim_name);

    let copied = copy_if_newer(shim_path, &dest_shim).map_err(|e| {
        BoxliteError::Storage(format!(
            "Failed to copy shim {} to {}: {}",
            shim_path.display(),
            dest_shim.display(),
            e
        ))
    })?;

    if copied {
        tracing::debug!(
            src = %shim_path.display(),
            dst = %dest_shim.display(),
            "Copied shim binary to box directory"
        );
    }

    // Copy all packaged firmware artifacts used by either runtime path.
    if let Some(shim_dir) = shim_path.parent() {
        copy_libkrunfw(shim_dir, &bin_dir)?;
    }

    Ok(dest_shim)
}

/// Copy libkrunfw from the shim's directory to `dest_dir`.
///
/// Copies `libkrunfw.so*` for libkrun's `dlopen` fallback and `libkrunfw.bin`
/// for explicit `krun_set_kernel` configuration when those files are present.
/// Uses copy-if-newer to avoid unnecessary copies on subsequent starts.
fn copy_libkrunfw(src_dir: &Path, dest_dir: &Path) -> BoxliteResult<()> {
    let entries = match std::fs::read_dir(src_dir) {
        Ok(entries) => entries,
        Err(e) => {
            tracing::warn!(
                src_dir = %src_dir.display(),
                error = %e,
                "Could not read source directory for libkrunfw"
            );
            return Ok(());
        }
    };

    for entry in entries.filter_map(|e| e.ok()) {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if name_str.starts_with(LIBKRUNFW_PREFIX) {
            let src_path = entry.path();
            let dest_path = dest_dir.join(&name);

            let copied = copy_if_newer(&src_path, &dest_path).map_err(|e| {
                BoxliteError::Storage(format!(
                    "Failed to copy libkrunfw {} to {}: {}",
                    src_path.display(),
                    dest_path.display(),
                    e
                ))
            })?;

            if copied {
                tracing::debug!(
                    lib = %name_str,
                    dst = %dest_path.display(),
                    "Copied libkrunfw to box directory"
                );
            }
        }
    }

    Ok(())
}
