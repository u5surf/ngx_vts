# nginx-vts-rust

[![CI](https://github.com/u5surf/ngx_vts/actions/workflows/ci.yml/badge.svg)](https://github.com/u5surf/ngx_vts/actions/workflows/ci.yml)

A Rust implementation of nginx-module-vts for virtual host traffic status
monitoring, built on top of the [ngx-rust][ngx-rust] framework.

**Status:** experimental, but the cross-worker aggregation path, the
`vts_zone` directive, and the `/status` endpoint are working end-to-end
with nginx 1.31.

[ngx-rust]: https://github.com/nginx/ngx-rust

## Architecture overview

```
┌────────────────────────────────────────────────────────────────────┐
│ nginx master                                                       │
│                                                                    │
│  vts_zone main 1m;  ─►  ngx_shared_memory_add  ─►  shm_zone        │
│                                                       │            │
│                                                       ▼            │
│                                       vts_init_shm_zone (Rust)     │
│                                                       │            │
│                                                       ▼            │
│           ┌──────────────────────────────────────────────┐         │
│           │ slab pool                                    │         │
│           │   ┌─ VtsSharedContext { table, shpool }      │         │
│           │   └─ VtsSharedTable                          │         │
│           │        ├ [VtsServerSlot;   256]              │         │
│           │        └ [VtsUpstreamSlot; 512]              │         │
│           └──────────────────────────────────────────────┘         │
└─────────────────────────────┬──────────────────────────────────────┘
                              │  fork
              ┌───────────────┴────────────────┐
              ▼                                ▼
   ┌──────────────────────┐         ┌──────────────────────┐
   │ worker 1             │         │ worker 2             │
   │  LOG_PHASE handler   │         │  LOG_PHASE handler   │
   │   └─► record_server  │         │   └─► record_server  │
   │   └─► record_upstr.  │ ──┬───► │   └─► record_upstr.  │
   │  /status handler     │   │     │  /status handler     │
   │   └─► snapshot_*     │   │     │   └─► snapshot_*     │
   └──────────────────────┘   │     └──────────────────────┘
                              │
                  ngx_shmtx_lock(&shpool->mutex)
                  guards every read & every write
```

The shared table is a fixed-layout `#[repr(C)]` struct of bounded size
(roughly 240 KB), so no slab allocation happens at request time, the
slot count caps the per-zone cardinality (Host-header DoS surface is
gone), and the layout maps cleanly onto raw shared memory.

When `vts_zone` is **not** declared the FFI transparently falls back to
a process-local manager — this is how the unit tests exercise the data
model without nginx.

## Features

- **Cross-worker aggregation** — every worker writes to the same slab
  table; `/status` returns totals across the whole nginx instance.
- **`vts_zone` directive** — declares a real shared-memory zone
  (`ngx_shared_memory_add`) whose `init` callback installs the fixed
  layout from Rust.
- **Server-zone metrics** keyed by the matched server block's first
  `server_name` (not the raw `Host` header), so the table can't be
  blown up by adversarial Host values.
- **Upstream metrics** per `(upstream, server)` peer — request counts,
  bytes in/out, status-code class buckets, request and upstream
  response times.
- **Prometheus text format** at `/status`.
- **Reload-safe** — `nginx -s reload` reuses the existing shared table,
  so counters survive a config reload.
- **Connection metrics** — currently approximated; a proper read of
  `ngx_stat_*` is on the open-issues list.

## Build

### Requirements

- Rust 1.85 or later (ngx-rust 0.5 uses edition 2024).
- nginx source tree (any 1.24+ release; CI is pinned to 1.28.0).
- A C compiler (`cc` / `clang`).
- pcre2 and zlib headers for the nginx build.

### Build the Rust cdylib

```bash
export NGINX_SOURCE_DIR=/path/to/nginx-source     # ngx-rust looks here
cargo build --release
```

Output: `target/release/libngx_vts_rust.{so,dylib}`.

### Build nginx with the module

```bash
cd /path/to/nginx-source
auto/configure --prefix=/tmp/nginx-vts-test \
               --with-compat \
               --add-dynamic-module=/path/to/ngx_vts
make
```

This produces:
- `objs/nginx` — the nginx binary (only needed if you don't already
  have one built from the same source).
- `objs/ngx_http_vts_module.so` — the dynamic module you load from
  `nginx.conf` via `load_module`.

The repository's `config` script picks `.dylib` on macOS and `.so` on
Linux automatically.

## Quick start

Minimal `nginx.conf` that proxies through an upstream and exposes
`/status`:

```nginx
load_module modules/ngx_http_vts_module.so;

events {
    worker_connections 64;
}

http {
    vts_zone main 1m;

    upstream backend {
        server 127.0.0.1:18091;
        server 127.0.0.1:18092;
    }

    # Two local servers acting as the upstream peers.
    server { listen 18091; location / { return 200 "peer1\n"; } }
    server { listen 18092; location / { return 200 "peer2\n"; } }

    server {
        listen 18080;
        server_name example.test;

        location /         { proxy_pass http://backend; }
        location /status   { vts_status; allow 127.0.0.1; deny all; }
    }
}
```

Run it:

```bash
mkdir -p /tmp/nginx-vts-test/{conf,logs,modules}
cp objs/ngx_http_vts_module.so /tmp/nginx-vts-test/modules/
cp nginx.conf                  /tmp/nginx-vts-test/conf/
objs/nginx -p /tmp/nginx-vts-test -c conf/nginx.conf
```

Drive traffic and read the metrics:

```bash
$ seq 1 100 | xargs -P 8 -I{} curl -sS -o /dev/null http://127.0.0.1:18080/
$ curl -sS http://127.0.0.1:18080/status
```

### Sample output (verbatim, after 105 proxied requests across 2 workers)

```
# nginx-vts-rust
# Version: 0.1.0
# Hostname: …
# Current Time: 1779530713

# VTS Status: Active
# Module: nginx-vts-rust

# Prometheus Metrics:
# HELP nginx_vts_info Nginx VTS module information
# TYPE nginx_vts_info gauge
nginx_vts_info{hostname="…",version="0.1.0"} 1

# HELP nginx_vts_connections Current nginx connections
# TYPE nginx_vts_connections gauge
nginx_vts_connections{state="active"} 8
nginx_vts_connections{state="reading"} 3
nginx_vts_connections{state="writing"} 3
nginx_vts_connections{state="waiting"} 2

# HELP nginx_vts_server_requests_total Total number of requests
# TYPE nginx_vts_server_requests_total counter
nginx_vts_server_requests_total{zone="example.test"} 105

# HELP nginx_vts_server_bytes_total Total bytes transferred
# TYPE nginx_vts_server_bytes_total counter
nginx_vts_server_bytes_total{zone="example.test",direction="in"}  8190
nginx_vts_server_bytes_total{zone="example.test",direction="out"} 16065

# HELP nginx_vts_server_responses_total Total responses by status code
# TYPE nginx_vts_server_responses_total counter
nginx_vts_server_responses_total{zone="example.test",status="2xx"} 105
…

# HELP nginx_vts_upstream_requests_total Total upstream requests
# TYPE nginx_vts_upstream_requests_total counter
nginx_vts_upstream_requests_total{upstream="backend",server="127.0.0.1:18091"} 53
nginx_vts_upstream_requests_total{upstream="backend",server="127.0.0.1:18092"} 52

# HELP nginx_vts_upstream_responses_total Upstream responses by status code
# TYPE nginx_vts_upstream_responses_total counter
nginx_vts_upstream_responses_total{upstream="backend",server="127.0.0.1:18091",status="2xx"} 53
nginx_vts_upstream_responses_total{upstream="backend",server="127.0.0.1:18092",status="2xx"} 52

# HELP nginx_vts_upstream_server_up Upstream server status (1=up, 0=down)
# TYPE nginx_vts_upstream_server_up gauge
nginx_vts_upstream_server_up{upstream="backend",server="127.0.0.1:18091"} 1
nginx_vts_upstream_server_up{upstream="backend",server="127.0.0.1:18092"} 1
```

Note that `peer1 (53) + peer2 (52) = 105`: both workers feed the same
table, so `/status` shows the totals regardless of which worker
happened to handle the request.

## Directives

| Directive | Context | Args | Description |
|-----------|---------|------|-------------|
| `vts_zone` | `http` | `name size` | Declare the shared-memory zone backing all counters. Minimum size is 1 MB; without this directive the module silently falls back to process-local counters (mainly useful for tests). |
| `vts_status` | `location` | — | Render the Prometheus text response at this location. |
| `vts_upstream_stats` | `http`, `server`, `location` | `on \| off` | Accepted for backward compatibility; currently a no-op (upstream stats are always collected when `vts_zone` is set). |

## Capacity

The shared state is two `RbTreeMap`s — one keyed by `server_name`, one
keyed by the `(upstream, server)` pair — allocated inside the slab pool
that backs the `vts_zone`. There is no compile-time slot cap: how many
distinct keys you can track is bounded only by the slab pool size you
configure with `vts_zone <name> <size>`.

Rough sizing rule of thumb: a `1m` zone comfortably holds a few thousand
server-zone keys plus a few thousand upstream pairs. Each entry is on the
order of ~200 bytes for the counters plus the key length plus rbtree
node overhead. Bump the size if you genuinely have more virtual hosts.

When a new key cannot be allocated (the slab pool is full), it is dropped
silently and existing counters keep updating. There is also a defensive
upper bound on key length (`VTS_MAX_KEY_BYTES = 256`) to keep
misconfigured `server_name` directives from chewing up the pool.

Keys are derived from nginx configuration (the matched server block's
first `server_name`, the upstream block name) — never from the raw `Host`
header — so attacker-controlled values cannot expand the key space.

## Development

### Tests

```bash
NGINX_SOURCE_DIR=/path/to/nginx-source cargo test --lib
```

There are 52 unit tests covering the shared-table data model, the
upstream tracker, the Prometheus formatter, the cache statistics
helpers, and the integration paths through `VTS_MANAGER`.

### Lints

```bash
NGINX_SOURCE_DIR=/path/to/nginx-source cargo clippy --all-targets -- -D warnings
cargo fmt --all -- --check
```

## What's not done yet

The list below tracks known gaps relative to the original
`nginx-module-vts`. None of them block normal traffic monitoring.

- JSON / HTML / JSONP output formats — only Prometheus text is emitted.
- `/control` API for reset/delete.
- Filter zones (`vhost_traffic_status_filter_by_set_key`).
- Cache statistics: the formatter is in place but there is no nginx
  feed wired to it yet (no LOG_PHASE call site reads
  `$upstream_cache_status`).
- Connection counters are read from `cycle->connections` rather than
  the `ngx_stat_*` atomics. `reading`/`writing`/`waiting` are
  therefore approximate.
- LOG_PHASE handler still logs at `NGX_LOG_NOTICE`; should be moved to
  debug level before any production use.
- Upstream peer state (`down`, `weight`, `max_fails`, …) is not yet
  read from the nginx upstream configuration.
- Multi-peer retries (`u->states`) are not iterated; only the last
  state is counted.

## License

Licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or
  http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or
  http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in the work by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
