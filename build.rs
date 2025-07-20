use std::env;
use std::path::PathBuf;

fn main() {
    // Set up nginx version from environment variable
    let ngx_version = env::var("NGX_VERSION").unwrap_or_else(|_| "1.24.0".to_string());
    println!("cargo:rustc-env=NGX_VERSION={}", ngx_version);

    // Enable debug if requested
    if env::var("NGX_DEBUG").is_ok() {
        println!("cargo:rustc-cfg=feature=\"debug\"");
    }

    // Platform-specific configurations
    if cfg!(target_os = "linux") {
        println!("cargo:rustc-cfg=feature=\"linux\"");
    }

    // Link against required nginx libraries
    println!("cargo:rustc-link-lib=dylib=nginx");
    
    // Specify library search paths if NGX_LIB_PATH is set
    if let Ok(lib_path) = env::var("NGX_LIB_PATH") {
        println!("cargo:rustc-link-search=native={}", lib_path);
    }
}