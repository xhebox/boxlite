//! External libkrunfw kernel configuration.

use super::context::KrunContext;
use crate::runtime::constants::envs::{
    BOXLITE_KRUNFW_KERNEL_FORMAT as ENV_KRUNFW_KERNEL_FORMAT,
    BOXLITE_KRUNFW_KERNEL_PATH as ENV_KRUNFW_KERNEL_PATH,
};
use boxlite_shared::errors::{BoxliteError, BoxliteResult};
use std::path::PathBuf;

const LIBKRUNFW_KERNEL_FILE: &str = "libkrunfw.bin";

/// A validated external kernel ready to be applied to a libkrun context.
pub(crate) struct KrunfwKernelConfig {
    path: PathBuf,
    format: u32,
}

impl KrunfwKernelConfig {
    pub(crate) fn from_env() -> BoxliteResult<Option<Self>> {
        let Some(path) = Self::find_path()? else {
            return Ok(None);
        };
        Ok(Some(Self {
            path,
            format: Self::format_from_env()?,
        }))
    }

    pub(crate) fn apply(&self, ctx: &KrunContext) -> BoxliteResult<()> {
        let path = self.path.to_str().ok_or_else(|| {
            BoxliteError::Config(format!(
                "krunfw kernel path contains invalid UTF-8: {}",
                self.path.display()
            ))
        })?;
        tracing::info!(
            kernel = %self.path.display(),
            format = self.format,
            "Configuring libkrun with krunfw kernel"
        );
        unsafe { ctx.set_kernel(path, self.format, None, None) }
    }

    fn parse_format(value: &str) -> BoxliteResult<u32> {
        match value.trim().to_ascii_lowercase().as_str() {
            "raw" => Ok(libkrun_sys::KRUN_KERNEL_FORMAT_RAW),
            "elf" => Ok(libkrun_sys::KRUN_KERNEL_FORMAT_ELF),
            "pe-gz" => Ok(libkrun_sys::KRUN_KERNEL_FORMAT_PE_GZ),
            "image-bz2" => Ok(libkrun_sys::KRUN_KERNEL_FORMAT_IMAGE_BZ2),
            "image-gz" => Ok(libkrun_sys::KRUN_KERNEL_FORMAT_IMAGE_GZ),
            "image-zstd" => Ok(libkrun_sys::KRUN_KERNEL_FORMAT_IMAGE_ZSTD),
            other => Err(BoxliteError::Config(format!(
                "unsupported {ENV_KRUNFW_KERNEL_FORMAT}={other}; use raw, elf, pe-gz, image-bz2, image-gz, or image-zstd"
            ))),
        }
    }

    fn default_format() -> u32 {
        if cfg!(target_arch = "x86_64") {
            libkrun_sys::KRUN_KERNEL_FORMAT_ELF
        } else {
            libkrun_sys::KRUN_KERNEL_FORMAT_RAW
        }
    }

    fn format_from_env() -> BoxliteResult<u32> {
        match std::env::var(ENV_KRUNFW_KERNEL_FORMAT) {
            Ok(value) => Self::parse_format(&value),
            Err(std::env::VarError::NotPresent) => Ok(Self::default_format()),
            Err(error) => Err(BoxliteError::Config(format!(
                "failed to read {ENV_KRUNFW_KERNEL_FORMAT}: {error}"
            ))),
        }
    }

    fn find_path() -> BoxliteResult<Option<PathBuf>> {
        if let Some(path) = match std::env::var(ENV_KRUNFW_KERNEL_PATH) {
            Ok(path) => Some(path),
            Err(std::env::VarError::NotPresent) => None,
            Err(error) => {
                return Err(BoxliteError::Config(format!(
                    "failed to read {ENV_KRUNFW_KERNEL_PATH}: {error}"
                )));
            }
        } {
            if path.is_empty() {
                return Err(BoxliteError::Config(format!(
                    "{ENV_KRUNFW_KERNEL_PATH} must not be empty"
                )));
            }
            let path = PathBuf::from(path);
            let metadata = std::fs::metadata(&path).map_err(|error| {
                BoxliteError::Config(format!(
                    "failed to access {ENV_KRUNFW_KERNEL_PATH}={}: {error}",
                    path.display()
                ))
            })?;
            if metadata.is_file() {
                return Ok(Some(path));
            }
            return Err(BoxliteError::Config(format!(
                "{ENV_KRUNFW_KERNEL_PATH} must point to a regular file: {}",
                path.display()
            )));
        }

        let mut candidates = Vec::new();
        if let Ok(exe) = std::env::current_exe()
            && let Some(dir) = exe.parent()
        {
            candidates.push(dir.join(LIBKRUNFW_KERNEL_FILE));
        }
        if let Ok(runtime_dir) = std::env::var("BOXLITE_RUNTIME_DIR")
            && !runtime_dir.is_empty()
        {
            candidates.extend(
                std::env::split_paths(&runtime_dir).map(|path| path.join(LIBKRUNFW_KERNEL_FILE)),
            );
        }
        if let Some(path) = candidates.into_iter().find(|path| path.is_file()) {
            return Ok(Some(path));
        }

        if cfg!(all(target_env = "musl", target_feature = "crt-static")) {
            return Err(BoxliteError::Config(format!(
                "static musl libkrun requires an external kernel; set {ENV_KRUNFW_KERNEL_PATH} or place {LIBKRUNFW_KERNEL_FILE} next to boxlite-shim or in BOXLITE_RUNTIME_DIR"
            )));
        }

        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_formats() {
        for (value, expected) in [
            ("raw", libkrun_sys::KRUN_KERNEL_FORMAT_RAW),
            ("elf", libkrun_sys::KRUN_KERNEL_FORMAT_ELF),
            ("pe-gz", libkrun_sys::KRUN_KERNEL_FORMAT_PE_GZ),
            ("image-bz2", libkrun_sys::KRUN_KERNEL_FORMAT_IMAGE_BZ2),
            ("image-gz", libkrun_sys::KRUN_KERNEL_FORMAT_IMAGE_GZ),
            ("image-zstd", libkrun_sys::KRUN_KERNEL_FORMAT_IMAGE_ZSTD),
        ] {
            assert_eq!(KrunfwKernelConfig::parse_format(value).unwrap(), expected);
        }
    }

    #[test]
    fn rejects_implicit_formats() {
        for value in ["", "auto", "pegz", "vmlinuz", "bzimage"] {
            assert!(KrunfwKernelConfig::parse_format(value).is_err());
        }
    }
}
