# Integration tests

Ported from [nginx-module-vts](https://github.com/vozlt/nginx-module-vts)'s
suite, using the same framework (`Test::Nginx::Socket`) so the two can be read
side by side.

## Running them

The module has to be built into the nginx under test, and that nginx has to be
named:

```sh
cargo build --release
cd /path/to/nginx-source
auto/configure --with-compat --with-debug --add-module=/path/to/ngx_vts
make

cd /path/to/ngx_vts
TEST_NGINX_BINARY=/path/to/nginx-source/objs/nginx prove t/
```

`Test::Nginx::Socket` comes from CPAN:

```sh
cpanm --notest Test::Nginx
```

## What is covered

| file | ported from | what it checks |
| --- | --- | --- |
| `001.status_prometheus.t` | `022.display_prometheus.t` | `/status` serves Prometheus text; `HELP`/`TYPE` on every family |
| `002.server_zones.t` | `001.display_json.t`, `030.display_html_uri.t` | request, byte and status-class counters per server zone; an unknown `Host` does not create a zone |
| `003.upstream_peers.t` | `024.upstream_check.t`, `027.upstream_zone_peers.t` | per-peer request, byte and status-class counters; every peer of a group gets its own series |
| `004.histogram_buckets.t` | `023.histogram_buckets.t` | `_bucket{le=…}`, `+Inf`, `_sum`, `_count`, and the `histogram` type declaration |

## What is not covered, and why

The original suite has 46 files. Most of them exercise directives this module
does not have, so there is nothing to port rather than something left undone:

| original tests | why they do not apply |
| --- | --- |
| `000`, `030`, `031` HTML display | this module emits Prometheus only |
| `001`, `002`, `025`, `042` JSON display, schema, dump | as above |
| `003`–`005`, `021`, `032`, `034`, `039` filters | no `vhost_traffic_status_filter_*` directives |
| `006`–`014`, `029`, `036`, `038` control interface | no `vhost_traffic_status_control` |
| `015`, `016` Lua integration | no Lua bindings |
| `017`–`019` traffic limiting | no `vhost_traffic_status_limit_*` |
| `020` sum key | no `vhost_traffic_status_sum_key` |
| `028`, `040`, `041` upstream resolve and backup | `resolve` and backup peers are not tracked separately yet |
| `033` shm free size | no such metric |
| `043` variables | no `$vts_*` variables |

Two are worth porting once the behaviour exists: `045.upstream_next_attempts.t`
covers per-attempt upstream accounting, which this module claims to do, and
`026.long_names.t` and `037.upstream_long_peer_name.t` cover key lengths in the
slab.
