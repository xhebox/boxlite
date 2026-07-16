// Linker config for the shim binary. boxlite is rlib-only post-#494, so
// link-args targeted at bins must be emitted from this crate, not boxlite's
// build.rs.
fn main() {
    // rpath: shim dlopen's libkrunfw.<X>.dylib (collected next to it in
    // the runtime directory) at runtime via libkrun's loader.
    #[cfg(target_os = "macos")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,@loader_path");
    #[cfg(target_os = "linux")]
    println!("cargo:rustc-link-arg=-Wl,-rpath,$ORIGIN");
}
