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
| `005.cache_zones.t` | the cache half of `002.check_json_syntax.t` | MISS then HIT per cache zone, the full set of cache statuses, `max` and `used` size gauges, two zones counted apart |
| `006.counter_accumulation.t` | the non-dump half of `042.dump.t` | counters keep running totals across requests rather than reporting the last one |

## What is not ported, and why

The original suite has 46 files. Read by name they look like display tests;
several of them are not, so the reasons differ.

### Directives this module does not have

Nothing to port rather than something left undone.

| original tests | missing directive |
| --- | --- |
| `003`–`005`, `021`, `032`, `034`, `039` | `vhost_traffic_status_filter_*` |
| `006`–`014`, `029`, `036`, `038` | `vhost_traffic_status_control` |
| `015`, `016` | Lua bindings |
| `017`–`019` | `vhost_traffic_status_limit_*` |
| `020` | `vhost_traffic_status_display_sum_key` |
| `026` (partly) | `vhost_traffic_status_measure_status_codes`, histogram bucket configuration |
| `028`, `040`, `041` | `resolve` and backup peer tracking |
| `042` (partly) | `vhost_traffic_status_dump` |

### Genuinely about the output format

| original tests | what they check |
| --- | --- |
| `000`, `001` | the `Content-Type` of `/status/format/html` and `/json` |
| `030` | routing of the `/status/format/...` URIs |
| `031` | the rate figures in the HTML view |
| `025` | the JSON body against a schema |

### Features this module is missing

These read like display tests but are not, and are worth treating as gaps.

**`033.shm_free_size.t`** — not a JSON test at all: its second block asserts a
Prometheus series, `nginx_vts_main_shm_usage_bytes{shared="free_size"}`. Its
header explains why the metric exists: `usedSize` is the sum of node sizes,
which is not what the zone has spent, because the slab hands out a whole page
or slot per node. A zone can refuse a node while `usedSize` still reads well
below `maxSize`, and `freeSize` is the only thing that says whether there is
room. This module exposes nothing about its shared zone, so an operator cannot
tell a full zone from an idle one.

**`043.variables.t`** — exposes the same counters a second way, as `$vts_*`
request variables, and checks them through response headers rather than the
display. There is no equivalent surface here.

**`026.long_names.t`**, **`037.upstream_long_peer_name.t`** — key lengths in
the slab. The filter parts do not apply, but nothing here establishes what
happens to a very long `server_name` or peer address.

**`045.upstream_next_attempts.t`** — each attempt of a request that
`proxy_next_upstream` passed on is counted against the peer it was passed on
from, rather than only the peer that answered. This module's README claims that
behaviour, so this one should be portable; it is the first to write next.
