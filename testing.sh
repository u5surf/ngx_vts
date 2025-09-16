#!/bin/sh
export NGINX_SOURCE_DIR=/home/u5surf/nginx
cargo build --release
cargo test --lib --verbose
cargo clippy --all-targets --all-features -- -D warnings
cargo fmt
