//! BoxLite libkrun facade and libkrunfw sidecar constants.
//!
//! With the `krun` feature enabled, this crate forwards BoxLite's existing
//! `libkrun_sys::krun_*` call surface to the vendored Rust `libkrun` crate.
//! Build-time sidecar handling for `libkrunfw` lives in `build.rs`.

#[cfg(feature = "krun")]
use std::os::raw::c_char;

// Disk format constants from libkrun.h
pub const KRUN_DISK_FORMAT_RAW: u32 = 0;
pub const KRUN_DISK_FORMAT_QCOW2: u32 = 1;

// Kernel format constants from libkrun.h
pub const KRUN_KERNEL_FORMAT_RAW: u32 = 0;
pub const KRUN_KERNEL_FORMAT_ELF: u32 = 1;
pub const KRUN_KERNEL_FORMAT_PE_GZ: u32 = 2;
pub const KRUN_KERNEL_FORMAT_IMAGE_BZ2: u32 = 3;
pub const KRUN_KERNEL_FORMAT_IMAGE_GZ: u32 = 4;
pub const KRUN_KERNEL_FORMAT_IMAGE_ZSTD: u32 = 5;

#[cfg(feature = "krun")]
pub unsafe fn krun_create_ctx() -> i32 {
    krun::krun_create_ctx()
}

#[cfg(feature = "krun")]
pub unsafe fn krun_free_ctx(ctx_id: u32) -> i32 {
    krun::krun_free_ctx(ctx_id)
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_vm_config(ctx_id: u32, num_vcpus: u8, ram_mib: u32) -> i32 {
    krun::krun_set_vm_config(ctx_id, num_vcpus, ram_mib)
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_root(ctx_id: u32, root_path: *const c_char) -> i32 {
    unsafe { krun::krun_set_root(ctx_id, root_path) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_add_virtiofs(
    ctx_id: u32,
    mount_tag: *const c_char,
    host_path: *const c_char,
) -> i32 {
    unsafe { krun::krun_add_virtiofs(ctx_id, mount_tag, host_path) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_add_virtiofs3(
    ctx_id: u32,
    mount_tag: *const c_char,
    host_path: *const c_char,
    shm_size: u64,
    read_only: bool,
) -> i32 {
    unsafe { krun::krun_add_virtiofs3(ctx_id, mount_tag, host_path, shm_size, read_only) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_kernel(
    ctx_id: u32,
    kernel_path: *const c_char,
    kernel_format: u32,
    initramfs: *const c_char,
    cmdline: *const c_char,
) -> i32 {
    unsafe { krun::krun_set_kernel(ctx_id, kernel_path, kernel_format, initramfs, cmdline) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_exec(
    ctx_id: u32,
    exec_path: *const c_char,
    argv: *const *const c_char,
    envp: *const *const c_char,
) -> i32 {
    unsafe { krun::krun_set_exec(ctx_id, exec_path, argv, envp) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_env(ctx_id: u32, envp: *const *const c_char) -> i32 {
    unsafe { krun::krun_set_env(ctx_id, envp) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_workdir(ctx_id: u32, workdir_path: *const c_char) -> i32 {
    unsafe { krun::krun_set_workdir(ctx_id, workdir_path) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_split_irqchip(ctx_id: u32, enable: bool) -> i32 {
    krun::krun_split_irqchip(ctx_id, enable)
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_nested_virt(ctx_id: u32, enabled: bool) -> i32 {
    unsafe { krun::krun_set_nested_virt(ctx_id, enabled) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_gpu_options(ctx_id: u32, virgl_flags: u32) -> i32 {
    unsafe { krun::krun_set_gpu_options(ctx_id, virgl_flags) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_rlimits(ctx_id: u32, rlimits: *const *const c_char) -> i32 {
    unsafe { krun::krun_set_rlimits(ctx_id, rlimits) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_port_map(ctx_id: u32, port_map: *const *const c_char) -> i32 {
    unsafe { krun::krun_set_port_map(ctx_id, port_map) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_add_vsock_port2(
    ctx_id: u32,
    port: u32,
    filepath: *const c_char,
    listen: bool,
) -> i32 {
    unsafe { krun::krun_add_vsock_port2(ctx_id, port, filepath, listen) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_add_disk(
    ctx_id: u32,
    block_id: *const c_char,
    disk_path: *const c_char,
    read_only: bool,
) -> i32 {
    unsafe { krun::krun_add_disk(ctx_id, block_id, disk_path, read_only) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_add_disk2(
    ctx_id: u32,
    block_id: *const c_char,
    disk_path: *const c_char,
    disk_format: u32,
    read_only: bool,
) -> i32 {
    unsafe { krun::krun_add_disk2(ctx_id, block_id, disk_path, disk_format, read_only) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_add_net_unixstream(
    ctx_id: u32,
    c_path: *const c_char,
    fd: i32,
    c_mac: *const u8,
    features: u32,
    flags: u32,
) -> i32 {
    unsafe { krun::krun_add_net_unixstream(ctx_id, c_path, fd, c_mac, features, flags) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_add_net_unixgram(
    ctx_id: u32,
    c_path: *const c_char,
    fd: i32,
    c_mac: *const u8,
    features: u32,
    flags: u32,
) -> i32 {
    unsafe { krun::krun_add_net_unixgram(ctx_id, c_path, fd, c_mac, features, flags) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_disable_implicit_vsock(ctx_id: u32) -> i32 {
    krun::krun_disable_implicit_vsock(ctx_id)
}

#[cfg(feature = "krun")]
pub unsafe fn krun_add_vsock(ctx_id: u32, tsi_features: u32) -> i32 {
    krun::krun_add_vsock(ctx_id, tsi_features)
}

#[cfg(feature = "krun")]
pub unsafe fn krun_start_enter(ctx_id: u32) -> i32 {
    krun::krun_start_enter(ctx_id)
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_console_output(ctx_id: u32, filepath: *const c_char) -> i32 {
    unsafe { krun::krun_set_console_output(ctx_id, filepath) }
}

#[cfg(feature = "krun")]
pub unsafe fn krun_setuid(ctx_id: u32, uid: libc::uid_t) -> i32 {
    krun::krun_setuid(ctx_id, uid)
}

#[cfg(feature = "krun")]
pub unsafe fn krun_setgid(ctx_id: u32, gid: libc::gid_t) -> i32 {
    krun::krun_setgid(ctx_id, gid)
}

#[cfg(feature = "krun")]
pub unsafe fn krun_set_root_disk_remount(
    ctx_id: u32,
    device: *const c_char,
    fstype: *const c_char,
    options: *const c_char,
) -> i32 {
    unsafe { krun::krun_set_root_disk_remount(ctx_id, device, fstype, options) }
}
