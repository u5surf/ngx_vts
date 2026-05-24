# syntax=docker/dockerfile:1.7
#
# Multi-stage build of nginx + ngx_vts_rust for the examples stack.
# Build context is the repository root (see docker-compose.yml).

ARG NGINX_VERSION=1.31.1

# ---------- Stage 1: build the Rust cdylib ----------
FROM rust:1.85-bookworm AS rust-build

ARG NGINX_VERSION

# nginx-sys runs ./configure during the Rust build (so we need the
# same toolchain bits nginx itself needs), and bindgen needs libclang.
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        gcc \
        libclang-dev \
        libpcre2-dev \
        zlib1g-dev \
        wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
RUN wget -q "https://nginx.org/download/nginx-${NGINX_VERSION}.tar.gz" \
    && tar xzf "nginx-${NGINX_VERSION}.tar.gz" \
    && mv "nginx-${NGINX_VERSION}" nginx-src \
    && rm "nginx-${NGINX_VERSION}.tar.gz" \
    && cd nginx-src && ./configure --with-compat >/dev/null

WORKDIR /build/ngx_vts_rust
COPY Cargo.toml Cargo.lock ./
COPY config config.make ./
COPY src ./src

ENV NGINX_SOURCE_DIR=/build/nginx-src
RUN cargo build --release

# ---------- Stage 2: build nginx with the dynamic module ----------
FROM debian:bookworm AS nginx-build

ARG NGINX_VERSION

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates \
        gcc \
        make \
        libpcre2-dev \
        zlib1g-dev \
        wget \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
RUN wget -q "https://nginx.org/download/nginx-${NGINX_VERSION}.tar.gz" \
    && tar xzf "nginx-${NGINX_VERSION}.tar.gz" \
    && mv "nginx-${NGINX_VERSION}" nginx-src \
    && rm "nginx-${NGINX_VERSION}.tar.gz"

# Module sources (C wrapper + module entrypoint) plus the prebuilt
# cdylib from Stage 1.  Layout must match what the addon `config`
# script expects: `target/release/libngx_vts_rust.so` relative to the
# addon directory.
WORKDIR /build/ngx_vts_rust
COPY config config.make ./
COPY src ./src
COPY --from=rust-build \
    /build/ngx_vts_rust/target/release/libngx_vts_rust.so \
    ./target/release/libngx_vts_rust.so

WORKDIR /build/nginx-src
# `--with-http_stub_status_module` is what defines `NGX_STAT_STUB`,
# which in turn populates the `ngx_stat_*` atomics that the Rust
# side reads for accurate connection-state counters.  Without it
# the build still works, but `nginx_vts_connections{state="..."}`
# falls back to a coarse approximation.
RUN ./configure \
        --prefix=/etc/nginx \
        --sbin-path=/usr/sbin/nginx \
        --conf-path=/etc/nginx/nginx.conf \
        --error-log-path=/var/log/nginx/error.log \
        --http-log-path=/var/log/nginx/access.log \
        --pid-path=/run/nginx.pid \
        --modules-path=/etc/nginx/modules \
        --with-compat \
        --with-http_stub_status_module \
        --add-dynamic-module=/build/ngx_vts_rust \
    && make -j"$(nproc)"

# ---------- Stage 3: runtime ----------
FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        libpcre2-8-0 \
        zlib1g \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --system nginx \
    && useradd --system -g nginx -d /var/cache/nginx -s /sbin/nologin nginx

COPY --from=nginx-build /build/nginx-src/objs/nginx /usr/sbin/nginx
COPY --from=nginx-build \
    /build/nginx-src/objs/ngx_http_vts_module.so \
    /etc/nginx/modules/ngx_http_vts_module.so
# The dynamic module records the cdylib's build path as its NEEDED
# entry, so the runtime image must expose it at the same path.
COPY --from=rust-build \
    /build/ngx_vts_rust/target/release/libngx_vts_rust.so \
    /build/ngx_vts_rust/target/release/libngx_vts_rust.so

RUN mkdir -p /etc/nginx /var/log/nginx /var/cache/nginx/zone1 /run \
    && chown -R nginx:nginx /var/cache/nginx

COPY examples/nginx/nginx.conf /etc/nginx/nginx.conf

EXPOSE 8080
STOPSIGNAL SIGQUIT
CMD ["nginx", "-g", "daemon off;"]
