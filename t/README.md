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
| `007.shm_usage.t` | `033.shm_free_size.t` | the zone reports its configured size, what the slab has left, and how many entries it holds |
| `008.upstream_next_attempts.t` | `045.upstream_next_attempts.t` | a request that `proxy_next_upstream` passed on is counted against the peer it was passed on from, with that attempt's own status and no bytes |
| `009.upstream_resolve.t` | `028.upstream_resolve.t` | peers resolved at run time are counted, and a peer replaced by a re-resolve keeps its numbers |

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
| `040`, `041` | backup peer tracking after a re-resolve |
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

**`043.variables.t`** — exposes the same counters a second way, as `$vts_*`
request variables, and checks them through response headers rather than the
display. There is no equivalent surface here.

**`026.long_names.t`**, **`037.upstream_long_peer_name.t`** — key lengths in
the slab. The filter parts do not apply, but nothing here establishes what
happens to a very long `server_name` or peer address.

**`029.control_upstream_addrs.t`**, **`038.control_resolve_peers.t`** — the
control interface over resolved peers. No control interface here.

## Two differences worth knowing

Both came out of porting `028` and `045`, and neither is a defect so much as a
consequence of how this module collects its numbers.

**Peers appear only once they have served.** The original enumerates the
servers of a group, so every configured peer has an entry from the start, with
zeroes. This module writes a series when a peer first appears in
`r->upstream_states`, so a peer that has never been chosen is absent rather
than zero. After a re-resolve the new address shows up as soon as the balancer
sends it anything, not the moment the group changes.

**A peer that leaves the group is not marked.** The original sets `weight: 0`
on a peer the group no longer holds, which distinguishes it from one that is
merely idle. Here the series simply stays as it was. The numbers are kept
either way - `009.upstream_resolve.t` asserts that - but nothing says the
address is gone.
