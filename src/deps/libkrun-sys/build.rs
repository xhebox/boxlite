use std::collections::HashMap;
use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

// ── Constants ────────────────────────────────────────────────────────────────

// libkrunfw release configuration
// Source: https://github.com/boxlite-ai/libkrunfw (fork with prebuilt releases)
const LIBKRUNFW_VERSION: &str = "v5.3.0";

// macOS: Download prebuilt kernel.c, compile locally to .dylib
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const LIBKRUNFW_PREBUILT_URL: &str = "https://github.com/boxlite-ai/libkrunfw/releases/download/v5.3.0/libkrunfw-prebuilt-aarch64.tgz";
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
const LIBKRUNFW_SHA256: &str = "12b9401d7735d1682450e4d025273c5016ec2237dcbfb76b2f0a152be6e606d6";

// Linux: Download pre-compiled .so directly (no build needed)
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const LIBKRUNFW_SO_URL: &str =
    "https://github.com/boxlite-ai/libkrunfw/releases/download/v5.3.0/libkrunfw-x86_64.tgz";
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
const LIBKRUNFW_SHA256: &str = "0a7bb64a35a273b8501801dd69b75736a8c676aa21aa62fb5642842cda9dc91d";

#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const LIBKRUNFW_SO_URL: &str =
    "https://github.com/boxlite-ai/libkrunfw/releases/download/v5.3.0/libkrunfw-aarch64.tgz";
#[cfg(all(target_os = "linux", target_arch = "aarch64"))]
const LIBKRUNFW_SHA256: &str = "8b5b9211da5445d9301dafb2201431f4392ab96455512bce63a5cfbd33c49839";

// Library directory name differs by platform
#[cfg(target_os = "macos")]
const LIB_DIR: &str = "lib";
#[cfg(target_os = "linux")]
const LIB_DIR: &str = "lib64";

// ── Core utilities ───────────────────────────────────────────────────────────

#[cfg(target_os = "linux")]
fn target_is_static_musl() -> bool {
    env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("musl")
        && env::var("CARGO_CFG_TARGET_FEATURE")
            .unwrap_or_default()
            .split(',')
            .any(|enabled| enabled == "crt-static")
}

#[cfg(target_os = "linux")]
fn libkrunfw_make_config() -> (HashMap<String, String>, Vec<String>) {
    let gcc_available = Command::new("gcc")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if gcc_available {
        return (HashMap::new(), Vec::new());
    }

    let clang_available = Command::new("clang")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !clang_available {
        panic!("building libkrunfw from source requires gcc or clang");
    }

    (
        HashMap::from([("LLVM".to_string(), "1".to_string())]),
        vec!["LLVM=1".to_string()],
    )
}

#[cfg(target_os = "linux")]
fn kernel_target() -> (&'static str, &'static str) {
    match env::var("CARGO_CFG_TARGET_ARCH").ok().as_deref() {
        Some("x86_64") => ("vmlinux", "x86_64"),
        Some("aarch64") => ("arch/arm64/boot/Image", "arm64"),
        Some("riscv64") => ("arch/riscv/boot/Image", "riscv"),
        Some(arch) => panic!("Unsupported libkrunfw kernel target architecture: {arch}"),
        None => panic!("CARGO_CFG_TARGET_ARCH is not set"),
    }
}

#[cfg(target_os = "linux")]
fn configure_kernel_make_arch(make_args: &mut Vec<String>) {
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").expect("CARGO_CFG_TARGET_ARCH is not set");
    let host_arch = env::var("HOST")
        .expect("HOST is not set")
        .split('-')
        .next()
        .expect("HOST is empty")
        .to_string();
    if target_arch != host_arch {
        make_args.push(format!("ARCH={}", kernel_target().1));
    }
}

#[cfg(target_os = "linux")]
fn kernel_source_path(libkrunfw_src: &Path) -> PathBuf {
    let makefile = fs::read_to_string(libkrunfw_src.join("Makefile"))
        .expect("failed to read vendored libkrunfw Makefile");
    let kernel_version = makefile
        .lines()
        .find_map(|line| {
            line.strip_prefix("KERNEL_VERSION")
                .and_then(|value| value.split_once('='))
                .map(|(_, value)| value.trim())
        })
        .filter(|value| !value.is_empty())
        .expect("libkrunfw Makefile does not define KERNEL_VERSION");
    libkrunfw_src.join(kernel_version).join(kernel_target().0)
}

#[cfg(target_os = "linux")]
fn export_libkrunfw_kernel(libkrunfw_src: &Path, out_dir: &Path) {
    let kernel_src = kernel_source_path(libkrunfw_src);
    if !kernel_src.is_file() {
        panic!(
            "libkrunfw source build did not produce {}",
            kernel_src.display()
        );
    }

    let kernel_dest = out_dir.join("libkrunfw.bin");
    fs::copy(&kernel_src, &kernel_dest).unwrap_or_else(|error| {
        panic!(
            "Failed to copy krunfw kernel artifact {} -> {}: {}",
            kernel_src.display(),
            kernel_dest.display(),
            error
        )
    });
    println!("cargo:KERNEL_BOXLITE_DEP={}", kernel_dest.display());
}

#[cfg(target_os = "linux")]
fn build_libkrunfw_kernel(
    libkrunfw_src: &Path,
    make_env: &HashMap<String, String>,
    make_args: &[String],
) {
    let kernel_src = kernel_source_path(libkrunfw_src);
    let kernel_relative = kernel_src
        .strip_prefix(libkrunfw_src)
        .expect("libkrunfw kernel path must be inside the source directory");
    let mut make_cmd = make_command(libkrunfw_src, make_env);
    make_cmd.args(make_args).arg(kernel_relative);
    run_command(&mut make_cmd, "make libkrunfw kernel");
}

/// Runs a command and panics with a helpful message if it fails.
fn run_command(cmd: &mut Command, description: &str) {
    let status = cmd
        .status()
        .unwrap_or_else(|e| panic!("Failed to execute {}: {}", description, e));

    if !status.success() {
        panic!("{} failed with exit code: {:?}", description, status.code());
    }
}

/// Verifies vendored sources exist.
fn verify_vendored_sources(manifest_dir: &Path, require_libkrunfw: bool) {
    let libkrun_src = manifest_dir.join("vendor/libkrun");
    let libkrunfw_src = manifest_dir.join("vendor/libkrunfw");

    // Submodule directories can exist but be empty if `git submodule update` wasn't run.
    // Check for a marker file (Makefile) instead of just the directory.
    let missing_libkrun = !libkrun_src.join("Makefile").exists();
    let missing_libkrunfw = require_libkrunfw && !libkrunfw_src.join("Makefile").exists();

    if missing_libkrun || missing_libkrunfw {
        eprintln!("ERROR: Vendored sources not found");
        eprintln!();
        eprintln!("Initialize git submodules:");
        eprintln!("  git submodule update --init --recursive");
        std::process::exit(1);
    }
}

#[cfg(target_os = "linux")]
fn verify_pyelftools_available() {
    let available = Command::new("python3")
        .args(["-c", "import elftools.elf.elffile"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success());
    if !available {
        panic!(
            "dynamic libkrunfw source builds require python3 with pyelftools; install it with `python3 -m pip install pyelftools`"
        );
    }
}

// ── Fetcher: download, verify, extract ───────────────────────────────────────

struct Fetcher;

impl Fetcher {
    /// Downloads, verifies, and extracts a tarball.
    /// Skips download if tarball already exists at `tarball_path`.
    pub fn fetch(
        url: &str,
        sha256: &str,
        tarball_path: &Path,
        extract_dir: &Path,
    ) -> io::Result<()> {
        if !tarball_path.exists() {
            Self::download(url, tarball_path)?;
            Self::verify_sha256(tarball_path, sha256)?;
        }
        Self::extract_tarball(tarball_path, extract_dir)
    }

    /// Downloads a file from URL to the specified path.
    fn download(url: &str, dest: &Path) -> io::Result<()> {
        println!("cargo:warning=Downloading {}...", url);

        let output = Command::new("curl")
            .args(["-fsSL", "-o", dest.to_str().unwrap(), url])
            .output()?;

        if !output.status.success() {
            return Err(io::Error::other(format!(
                "curl failed: {}",
                String::from_utf8_lossy(&output.stderr)
            )));
        }

        Ok(())
    }

    /// Verifies SHA256 checksum of a file.
    fn verify_sha256(file: &Path, expected: &str) -> io::Result<()> {
        let (cmd, args): (&str, Vec<&str>) = if cfg!(target_os = "linux") {
            ("sha256sum", vec![file.to_str().unwrap()])
        } else {
            ("shasum", vec!["-a", "256", file.to_str().unwrap()])
        };

        let output = Command::new(cmd).args(&args).output()?;

        if !output.status.success() {
            return Err(io::Error::other(format!("{} failed", cmd)));
        }

        let actual = String::from_utf8_lossy(&output.stdout)
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();

        if actual != expected {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("SHA256 mismatch: expected {}, got {}", expected, actual),
            ));
        }

        println!("cargo:warning=SHA256 verified: {}", expected);
        Ok(())
    }

    /// Extracts a tarball to the specified directory.
    fn extract_tarball(tarball: &Path, dest: &Path) -> io::Result<()> {
        fs::create_dir_all(dest)?;

        let status = Command::new("tar")
            .args([
                "-xzf",
                tarball.to_str().unwrap(),
                "-C",
                dest.to_str().unwrap(),
            ])
            .status()?;

        if !status.success() {
            return Err(io::Error::other("tar extraction failed"));
        }

        Ok(())
    }
}

/// Downloads and extracts the prebuilt libkrunfw tarball (macOS).
/// Returns the path to the extracted source directory containing kernel.c.
#[cfg(target_os = "macos")]
fn download_libkrunfw_prebuilt(out_dir: &Path) -> PathBuf {
    let versioned_dir = format!("libkrunfw-src-{LIBKRUNFW_VERSION}");
    let tarball_path = out_dir.join(format!("libkrunfw-prebuilt-{LIBKRUNFW_VERSION}.tar.gz"));
    let extract_dir = out_dir.join(&versioned_dir);
    let src_dir = extract_dir.join("libkrunfw");

    if src_dir.join("kernel.c").exists() {
        println!("cargo:warning=Using cached libkrunfw source ({LIBKRUNFW_VERSION})");
        return src_dir;
    }

    // Clean stale extraction before re-extracting
    if extract_dir.exists() {
        fs::remove_dir_all(&extract_dir).ok();
    }

    Fetcher::fetch(
        LIBKRUNFW_PREBUILT_URL,
        LIBKRUNFW_SHA256,
        &tarball_path,
        &extract_dir,
    )
    .unwrap_or_else(|e| panic!("Failed to fetch libkrunfw: {}", e));

    println!("cargo:warning=Extracted libkrunfw to {}", src_dir.display());
    src_dir
}

/// Downloads pre-compiled libkrunfw .so files (Linux).
/// Extracts directly to the install directory - no build step needed.
#[cfg(target_os = "linux")]
fn download_libkrunfw_so(install_dir: &Path) {
    let lib_dir = install_dir.join(LIB_DIR);

    let version_marker = install_dir.join(format!(".version-{LIBKRUNFW_VERSION}"));
    if version_marker.exists() {
        println!("cargo:warning=Using cached libkrunfw.so ({LIBKRUNFW_VERSION})");
        return;
    }

    // Remove stale artifacts from a previous version
    if install_dir.exists() {
        fs::remove_dir_all(install_dir).ok();
    }

    fs::create_dir_all(install_dir)
        .unwrap_or_else(|e| panic!("Failed to create install dir: {}", e));

    let tarball_path = install_dir.join(format!("libkrunfw-{LIBKRUNFW_VERSION}.tgz"));

    Fetcher::fetch(
        LIBKRUNFW_SO_URL,
        LIBKRUNFW_SHA256,
        &tarball_path,
        install_dir,
    )
    .unwrap_or_else(|e| panic!("Failed to fetch libkrunfw: {}", e));

    fs::write(&version_marker, LIBKRUNFW_VERSION)
        .unwrap_or_else(|e| panic!("Failed to write version marker: {}", e));

    println!(
        "cargo:warning=Extracted libkrunfw.so to {}",
        lib_dir.display()
    );
}

// ── Make utilities ───────────────────────────────────────────────────────────

/// Creates a make command with common configuration.
fn make_command(source_dir: &Path, extra_env: &HashMap<String, String>) -> Command {
    let mut cmd = Command::new("make");
    cmd.stdout(Stdio::inherit());
    cmd.stderr(Stdio::inherit());
    cmd.args(["-j", &num_cpus::get().to_string()])
        .arg("MAKEFLAGS=") // Clear MAKEFLAGS to prevent -w flag issues in submakes
        .current_dir(source_dir);

    // Apply extra environment variables
    for (key, value) in extra_env {
        cmd.env(key, value);
    }

    cmd
}

/// Builds a library using Make with the specified parameters.
fn build_with_make(
    source_dir: &Path,
    install_dir: &Path,
    lib_name: &str,
    extra_env: &HashMap<String, String>,
    extra_make_args: &[String],
) {
    println!("cargo:warning=Building {} from source...", lib_name);

    fs::create_dir_all(install_dir)
        .unwrap_or_else(|e| panic!("Failed to create install directory: {}", e));

    // Build
    let mut make_cmd = make_command(source_dir, extra_env);
    make_cmd.env("PREFIX", install_dir);
    make_cmd.args(extra_make_args);
    run_command(&mut make_cmd, &format!("make {}", lib_name));

    // Install
    let mut install_cmd = make_command(source_dir, extra_env);
    install_cmd.env("PREFIX", install_dir);
    install_cmd.args(extra_make_args);
    install_cmd.arg("install");
    run_command(&mut install_cmd, &format!("make install {}", lib_name));
}

// ── LibFixup: post-build library fixup ───────────────────────────────────────

struct LibFixup;

impl LibFixup {
    /// Fixes the shared library name (install_name on macOS, SONAME on Linux).
    fn fix_install_name(lib_name: &str, lib_path: &Path) {
        let lib_path_str = lib_path.to_str().expect("Invalid library path");

        #[cfg(target_os = "macos")]
        let mut cmd = {
            let mut c = Command::new("install_name_tool");
            c.args(["-id", &format!("@rpath/{}", lib_name), lib_path_str]);
            c
        };

        #[cfg(target_os = "linux")]
        let mut cmd = {
            println!("cargo:warning=Fixing {} in {}", lib_name, lib_path_str);
            let mut c = Command::new("patchelf");
            c.args(["--set-soname", lib_name, lib_path_str]);
            c
        };

        run_command(&mut cmd, &format!("fix install name for {}", lib_name));
    }

    /// Extract SONAME from versioned library filename.
    /// e.g., libkrunfw.so.4.9.0 -> Some("libkrunfw.so.4")
    #[cfg(target_os = "linux")]
    fn extract_major_soname(filename: &str) -> Option<String> {
        if let Some(so_pos) = filename.find(".so.") {
            let base = &filename[..so_pos + 3];
            let versions = &filename[so_pos + 4..];

            if let Some(major) = versions.split('.').next() {
                return Some(format!("{}.{}", base, major));
            }
        }
        None
    }

    /// Fixes install names and re-signs libraries in a directory.
    pub fn fix(lib_dir: &Path, lib_prefix: &str) -> Result<(), String> {
        let ext = if cfg!(target_os = "macos") {
            ".dylib"
        } else {
            ".so"
        };

        for entry in
            fs::read_dir(lib_dir).map_err(|e| format!("Failed to read directory: {}", e))?
        {
            let entry = entry.map_err(|e| format!("Failed to read entry: {}", e))?;
            let path = entry.path();
            let filename = path.file_name().unwrap().to_string_lossy().to_string();

            if filename.starts_with(lib_prefix) && filename.contains(ext) {
                let metadata = fs::symlink_metadata(&path)
                    .map_err(|e| format!("Failed to get metadata: {}", e))?;

                if metadata.file_type().is_symlink() {
                    continue;
                }

                // Linux: rename libkrunfw to major-version soname
                #[cfg(target_os = "linux")]
                if lib_prefix == "libkrunfw" {
                    if let Some(soname) = Self::extract_major_soname(&filename) {
                        if soname != filename {
                            let new_path = lib_dir.join(&soname);
                            fs::rename(&path, &new_path)
                                .map_err(|e| format!("Failed to rename file: {}", e))?;
                            println!("cargo:warning=Renamed {} to {}", filename, soname);
                            Self::fix_install_name(&soname, &new_path);
                            continue;
                        }
                    }
                }

                Self::fix_install_name(&filename, &path);

                // macOS: re-sign after modifying
                #[cfg(target_os = "macos")]
                {
                    let sign_status = Command::new("codesign")
                        .args(["-s", "-", "--force"])
                        .arg(&path)
                        .status()
                        .map_err(|e| format!("Failed to run codesign: {}", e))?;

                    if !sign_status.success() {
                        return Err(format!("codesign failed for {}", filename));
                    }

                    println!("cargo:warning=Fixed and signed {}", filename);
                }
            }
        }

        Ok(())
    }
}

// ── MacToolchain: macOS toolchain discovery ──────────────────────────────────

#[cfg(target_os = "macos")]
struct MacToolchain {
    clang: PathBuf,
    path_dirs: Vec<PathBuf>,
}

#[cfg(target_os = "macos")]
impl MacToolchain {
    /// Sets LIBCLANG_PATH for bindgen if not already set.
    /// This is needed when llvm is installed via brew but not linked (keg-only).
    fn setup_libclang_path() {
        // Skip if LIBCLANG_PATH already set or llvm-config is in PATH
        if env::var("LIBCLANG_PATH").is_ok() {
            return;
        }
        if Command::new("llvm-config")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            return;
        }

        // Try common Homebrew locations (useful when `brew` itself can't be executed).
        for prefix in ["/opt/homebrew/opt/llvm", "/usr/local/opt/llvm"] {
            let lib_path = Path::new(prefix).join("lib");
            if lib_path.join("libclang.dylib").exists() {
                env::set_var("LIBCLANG_PATH", &lib_path);
                return;
            }
        }

        // Try to find brew's llvm
        if let Ok(output) = Command::new("brew").args(["--prefix", "llvm"]).output() {
            if output.status.success() {
                let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
                let lib_path = format!("{}/lib", prefix);
                if Path::new(&lib_path).join("libclang.dylib").exists() {
                    env::set_var("LIBCLANG_PATH", &lib_path);
                }
            }
        }
    }

    fn brew_prefix(formula: &str) -> Option<PathBuf> {
        let output = Command::new("brew")
            .args(["--prefix", formula])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }

        let prefix = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if prefix.is_empty() {
            return None;
        }

        Some(PathBuf::from(prefix))
    }

    fn find_non_apple_clang_in_path() -> Option<PathBuf> {
        let version = Command::new("clang").arg("--version").output().ok()?;
        if !version.status.success() {
            return None;
        }

        let version_stdout = String::from_utf8_lossy(&version.stdout);
        if version_stdout.starts_with("Apple clang") {
            return None;
        }

        let output = Command::new("which").arg("clang").output().ok()?;
        if !output.status.success() {
            return None;
        }

        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if path.is_empty() {
            return None;
        }

        let path = PathBuf::from(path);
        path.exists().then_some(path)
    }

    fn find_llvm_clang() -> Option<PathBuf> {
        // If the user has already put a non-Apple clang first in PATH, prefer that.
        if let Some(clang) = Self::find_non_apple_clang_in_path() {
            return Some(clang);
        }

        // If llvm-config is available, use it.
        if let Ok(output) = Command::new("llvm-config").arg("--bindir").output() {
            if output.status.success() {
                let bindir = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !bindir.is_empty() {
                    let clang = PathBuf::from(bindir).join("clang");
                    if clang.exists() {
                        return Some(clang);
                    }
                }
            }
        }

        // Common Homebrew locations (useful when `brew` itself can't be executed).
        for prefix in ["/opt/homebrew/opt/llvm", "/usr/local/opt/llvm"] {
            let clang = Path::new(prefix).join("bin/clang");
            if clang.exists() {
                return Some(clang);
            }
        }

        // Homebrew llvm is keg-only; locate it via brew.
        Self::brew_prefix("llvm")
            .map(|prefix| prefix.join("bin/clang"))
            .filter(|clang| clang.exists())
    }

    fn find_lld_bin_dir() -> Option<PathBuf> {
        // If ld.lld is already in PATH, we're good.
        if Command::new("ld.lld")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|s| s.success())
        {
            return None;
        }

        // Common Homebrew locations (useful when `brew` itself can't be executed).
        for prefix in ["/opt/homebrew/opt/lld", "/usr/local/opt/lld"] {
            let ld_lld = Path::new(prefix).join("bin/ld.lld");
            if ld_lld.exists() {
                return ld_lld.parent().map(Path::to_path_buf);
            }
        }

        // Otherwise, try locating via Homebrew.
        let ld_lld = Self::brew_prefix("lld")
            .map(|prefix| prefix.join("bin/ld.lld"))
            .filter(|path| path.exists())?;

        ld_lld.parent().map(Path::to_path_buf)
    }

    fn prepend_path_dirs(path_dirs: &[PathBuf]) -> Option<String> {
        if path_dirs.is_empty() {
            return None;
        }

        let existing = env::var("PATH").unwrap_or_default();
        let mut merged = String::new();
        for dir in path_dirs {
            if merged.is_empty() {
                merged.push_str(&dir.to_string_lossy());
            } else {
                merged.push(':');
                merged.push_str(&dir.to_string_lossy());
            }
        }

        if existing.is_empty() {
            return Some(merged);
        }

        merged.push(':');
        merged.push_str(&existing);
        Some(merged)
    }

    /// Discovers the LLVM clang and lld paths, storing them as intermediate state.
    fn discover() -> Result<Self, String> {
        if let Ok(cc_linux) = env::var("BOXLITE_LIBKRUN_CC_LINUX") {
            let cc_linux = cc_linux.trim().to_string();
            if cc_linux.is_empty() {
                return Err("BOXLITE_LIBKRUN_CC_LINUX is set but empty".to_string());
            }
            // User-provided override — no clang discovery needed, but we still
            // need a valid PathBuf. Store the raw string as the clang path.
            return Ok(Self {
                clang: PathBuf::from(cc_linux),
                path_dirs: Vec::new(),
            });
        }

        let clang = Self::find_llvm_clang().ok_or_else(|| {
            "libkrun cross-compilation on macOS requires LLVM clang + lld. Run `make setup` (or `brew install llvm lld`) and retry."
                .to_string()
        })?;

        let mut path_dirs = Vec::new();
        if let Some(dir) = clang.parent() {
            path_dirs.push(dir.to_path_buf());
        }
        if let Some(lld_dir) = Self::find_lld_bin_dir() {
            path_dirs.push(lld_dir);
        }

        Ok(Self { clang, path_dirs })
    }

    /// Converts the discovered toolchain into make arguments and env overrides.
    fn into_cc_linux(
        self,
        libkrun_src: &Path,
    ) -> Result<(String, HashMap<String, String>), String> {
        // If the user provided BOXLITE_LIBKRUN_CC_LINUX, return it directly
        if env::var("BOXLITE_LIBKRUN_CC_LINUX").is_ok() {
            let cc_linux = self.clang.to_string_lossy().to_string();
            return Ok((cc_linux, HashMap::new()));
        }

        let path_override = Self::prepend_path_dirs(&self.path_dirs);

        // Ensure ld.lld is available (either already in PATH or via brew lld).
        let mut ld_lld_cmd = Command::new("ld.lld");
        ld_lld_cmd
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(ref path) = path_override {
            ld_lld_cmd.env("PATH", path);
        }

        if !ld_lld_cmd.status().is_ok_and(|s| s.success()) {
            return Err(
                "Missing `ld.lld` (LLVM linker). Install it with `make setup` (or `brew install lld`)."
                    .to_string(),
            );
        }

        println!(
            "cargo:warning=Using LLVM clang for libkrun init cross-compile: {}",
            self.clang.display()
        );

        let linux_target_triple = match env::var("CARGO_CFG_TARGET_ARCH")
            .unwrap_or_else(|_| "aarch64".to_string())
            .as_str()
        {
            "arm64" | "aarch64" => "aarch64-linux-gnu".to_string(),
            "x86_64" => "x86_64-linux-gnu".to_string(),
            arch => format!("{arch}-linux-gnu"),
        };

        // Prepare sysroot via the Makefile's auto-download mechanism
        let sysroot_dir = libkrun_src.join("linux-sysroot");
        if !sysroot_dir.join(".sysroot_ready").exists() {
            println!("cargo:warning=Preparing Linux sysroot for cross-compilation...");
            let mut env_for_make = HashMap::new();
            if let Some(ref path) = path_override {
                env_for_make.insert("PATH".to_string(), path.clone());
            }
            let mut cmd = make_command(libkrun_src, &env_for_make);
            cmd.arg("linux-sysroot/.sysroot_ready");
            run_command(&mut cmd, "make linux-sysroot/.sysroot_ready");
        }

        let sysroot_abs = fs::canonicalize(&sysroot_dir)
            .unwrap_or_else(|e| panic!("Failed to resolve sysroot path: {}", e));

        let clang_str = self.clang.to_string_lossy();
        let cc_linux = format!(
            "{} -target {} -fuse-ld=lld -Wl,-strip-debug --sysroot {} -Wno-c23-extensions",
            clang_str,
            linux_target_triple,
            sysroot_abs.display()
        );

        let mut env_overrides = HashMap::new();
        if let Some(path) = path_override {
            env_overrides.insert("PATH".to_string(), path);
        }

        Ok((cc_linux, env_overrides))
    }

    /// Entry point: discovers the toolchain and produces CC_LINUX value + env overrides.
    pub fn resolve(libkrun_src: &Path) -> Result<(String, HashMap<String, String>), String> {
        Self::setup_libclang_path();
        Self::discover()?.into_cc_linux(libkrun_src)
    }
}

// ── Platform build orchestration ─────────────────────────────────────────────

/// Build and export the libkrunfw dylib when `krunfw` is enabled.
#[cfg(target_os = "macos")]
fn build() {
    if !cfg!(feature = "krunfw") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let libkrunfw_install = out_dir.join("libkrunfw");
    let libkrunfw_lib = libkrunfw_install.join(LIB_DIR);

    // Download and build libkrunfw dylib.
    println!("cargo:warning=Building libkrunfw for macOS...");
    let libkrunfw_src = download_libkrunfw_prebuilt(&out_dir);
    build_with_make(
        &libkrunfw_src,
        &libkrunfw_install,
        "libkrunfw",
        &HashMap::new(),
        &[],
    );
    LibFixup::fix(&libkrunfw_lib, "libkrunfw")
        .unwrap_or_else(|e| panic!("Failed to fix libkrunfw: {}", e));

    // Expose libkrunfw library directory for downstream bundling.
    println!("cargo:LIBKRUNFW_BOXLITE_DEP={}", libkrunfw_lib.display());
}

/// Build and export the libkrunfw artifacts selected by the enabled features
/// and target link mode.
#[cfg(target_os = "linux")]
fn build() {
    if !cfg!(feature = "krunfw") {
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let static_musl = target_is_static_musl();
    let build_from_source = cfg!(feature = "krunfw-source");

    if static_musl && !build_from_source {
        return;
    }

    if build_from_source {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        verify_vendored_sources(&manifest_dir, true);
        if !static_musl {
            verify_pyelftools_available();
        }
        let libkrunfw_src = manifest_dir.join("vendor/libkrunfw");
        let (make_env, mut make_args) = libkrunfw_make_config();
        configure_kernel_make_arch(&mut make_args);
        build_libkrunfw_kernel(&libkrunfw_src, &make_env, &make_args);

        if !static_musl {
            let libkrunfw_install = out_dir.join("libkrunfw");
            build_with_make(
                &libkrunfw_src,
                &libkrunfw_install,
                "libkrunfw",
                &make_env,
                &make_args,
            );
            let libkrunfw_lib_dir = libkrunfw_install.join(LIB_DIR);
            LibFixup::fix(&libkrunfw_lib_dir, "libkrunfw")
                .unwrap_or_else(|error| panic!("Failed to fix libkrunfw: {error}"));
            println!(
                "cargo:LIBKRUNFW_BOXLITE_DEP={}",
                libkrunfw_lib_dir.display()
            );
        }

        export_libkrunfw_kernel(&libkrunfw_src, &out_dir);
        return;
    }

    let libkrunfw_install = out_dir.join("libkrunfw");
    let libkrunfw_lib_dir = libkrunfw_install.join(LIB_DIR);
    download_libkrunfw_so(&libkrunfw_install);
    LibFixup::fix(&libkrunfw_lib_dir, "libkrunfw")
        .unwrap_or_else(|error| panic!("Failed to fix libkrunfw: {error}"));
    println!(
        "cargo:LIBKRUNFW_BOXLITE_DEP={}",
        libkrunfw_lib_dir.display()
    );
}

// ── Entry point ──────────────────────────────────────────────────────────────

fn main() {
    // Rebuild if vendored sources change
    println!("cargo:rerun-if-changed=vendor/libkrun");
    println!("cargo:rerun-if-changed=vendor/libkrunfw");
    println!("cargo:rerun-if-env-changed=BOXLITE_DEPS_STUB");
    #[cfg(target_os = "macos")]
    println!("cargo:rerun-if-env-changed=BOXLITE_LIBKRUN_CC_LINUX");

    // Auto-detect crates.io download: Cargo injects .cargo_vcs_info.json into
    // published packages. When present, enter stub mode since vendor sources are
    // excluded from the package and building from source is not possible.
    if env::var("BOXLITE_DEPS_STUB").is_err() {
        let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
        if manifest_dir.join(".cargo_vcs_info.json").exists() {
            // SAFETY: build.rs is single-threaded; no concurrent env var access.
            unsafe { env::set_var("BOXLITE_DEPS_STUB", "1") };
        }
    }

    // Check for stub mode (for CI linting or crates.io install)
    if env::var("BOXLITE_DEPS_STUB").is_ok() {
        println!("cargo:warning=BOXLITE_DEPS_STUB mode: skipping libkrun build");
        println!("cargo:LIBKRUNFW_BOXLITE_DEP=/nonexistent");
        return;
    }

    // Skip native artifact preparation when no build features are enabled.
    // The crate can still provide constants without libkrun/libkrunfw artifacts.
    let need_build = cfg!(feature = "krunfw") || cfg!(feature = "krun");
    if !need_build {
        println!("cargo:warning=libkrun-sys: no build features enabled, skipping native builds");
        return;
    }

    build();
}
