//! Allowlisted host environment inherited by the shim process.

/// Environment variables that may cross the shim's sanitized process boundary.
const SHIM_ENV_ALLOWLIST: &[&str] = &[
    "RUST_LOG",
    "RUST_BACKTRACE",
    crate::runtime::constants::envs::BOXLITE_KRUNFW_KERNEL_PATH,
    crate::runtime::constants::envs::BOXLITE_KRUNFW_KERNEL_FORMAT,
];

/// Returns allowlisted host environment variables for a shim subprocess.
pub(crate) fn shim_process_env() -> impl Iterator<Item = (&'static str, String)> {
    SHIM_ENV_ALLOWLIST
        .iter()
        .filter_map(|&name| std::env::var(name).ok().map(|value| (name, value)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowlist_contains_debug_controls() {
        assert!(SHIM_ENV_ALLOWLIST.contains(&"RUST_LOG"));
        assert!(SHIM_ENV_ALLOWLIST.contains(&"RUST_BACKTRACE"));
    }

    #[test]
    fn allowlist_contains_external_kernel_controls() {
        assert!(SHIM_ENV_ALLOWLIST.contains(&"BOXLITE_KRUNFW_KERNEL_PATH"));
        assert!(SHIM_ENV_ALLOWLIST.contains(&"BOXLITE_KRUNFW_KERNEL_FORMAT"));
    }
}
