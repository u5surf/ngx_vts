//! Re-publish the nginx feature/version probes that `nginx-sys` performs
//! so this crate can `#[cfg]` on them.
//!
//! Cargo only hands `DEP_NGINX_*` to *direct* dependents of the crate
//! that declares `links = "nginx"`, which is why `nginx-sys` is listed
//! in `Cargo.toml` alongside `ngx`.  See
//! <https://github.com/rust-lang/cargo/issues/3544>.
//!
//! Adapted from ngx-rust's `build.rs` example.

fn main() {
    // Specify acceptable values for `ngx_feature`
    println!("cargo::rerun-if-env-changed=DEP_NGINX_FEATURES_CHECK");
    println!(
        "cargo::rustc-check-cfg=cfg(ngx_feature, values({}))",
        std::env::var("DEP_NGINX_FEATURES_CHECK").unwrap_or_else(|_| "any()".to_string())
    );
    // Read feature flags detected by nginx-sys and pass them to the compiler.
    println!("cargo::rerun-if-env-changed=DEP_NGINX_FEATURES");
    if let Ok(features) = std::env::var("DEP_NGINX_FEATURES") {
        for feature in features.split(',').map(str::trim) {
            println!("cargo::rustc-cfg=ngx_feature=\"{feature}\"");
        }
    }

    // Specify acceptable values for `ngx_os`
    println!("cargo::rerun-if-env-changed=DEP_NGINX_OS_CHECK");
    println!(
        "cargo::rustc-check-cfg=cfg(ngx_os, values({}))",
        std::env::var("DEP_NGINX_OS_CHECK").unwrap_or_else(|_| "any()".to_string())
    );
    println!("cargo::rerun-if-env-changed=DEP_NGINX_OS");
    if let Ok(os) = std::env::var("DEP_NGINX_OS") {
        println!("cargo::rustc-cfg=ngx_os=\"{os}\"");
    }
}
