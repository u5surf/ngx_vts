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
| `010.upstream_backup_resolve.t` | `040.upstream_backup_resolve.t`, `041.upstream_backup_gone.t` | a backup peer the resolver made is counted against its address, and keeps its numbers when a re-resolve takes it out |
| `011.long_names.t` | `037.upstream_long_peer_name.t` | a unix socket peer keeps its whole path; a server name that fits is the zone, and one that does not falls back to the default rather than losing the request |
| `012.upstream_server_up.t` | `024.upstream_check.t` (the `down` half) | a peer is reported down when the configuration says so or when it has reached `max_fails`, up otherwise, and `max_fails 0` does not read as down |

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
| `026` | every block is about a filter name; `vhost_traffic_status_filter_by_set_key` and `vhost_traffic_status_measure_status_codes` |
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

**`029.control_upstream_addrs.t`**, **`038.control_resolve_peers.t`** — the
control interface over resolved peers. No control interface here.

## Two differences worth knowing

Both came out of porting `028` and `045`, and neither is a defect so much as a
consequence of how this module collects its numbers.

**Counters appear only once a peer has served.** The original enumerates the
servers of a group, so every configured peer has an entry from the start, with
zeroes. The counters here are written when a peer first appears in
`r->upstream_states`, so a peer that has never been chosen has no
`nginx_vts_upstream_requests_total` rather than a zero one. After a re-resolve
the new address gets counters as soon as the balancer sends it anything, not
the moment the group changes.

`nginx_vts_upstream_server_up` is the exception, and deliberately so: it comes
from a walk of the group itself (`src/peers.rs`) rather than from the counters,
because whether a peer is in rotation is not something a record of past
requests can answer. So an idle peer does get a `server_up` series -
`012.upstream_server_up.t` holds that - it just has no traffic counters beside
it yet.

This is why `040.upstream_backup_resolve.t` ports only in part: its blocks that
assert a peer is *named* before serving anything have no equivalent, and its
TEST 6, which reads a wide name's peers out of the display with no request made
at all, cannot pass here by construction.

It also means the bugs those two files were written for cannot occur here. Both
are failures of a reader that walks the upstream configuration - backup peers
made by a resolving line sit in `peers->next`, which neither of the original's
readers visited, and a group whose only resolving line is a backup was skipped
because `peers->resolve` was NULL. Reading `r->upstream_states` never has to
decide which list a peer came from.

**A peer that leaves the group is not marked.** The original sets `weight: 0`
on a peer the group no longer holds, which distinguishes it from one that is
merely idle. Here the counters simply stay as they were. The numbers are kept
either way - `009.upstream_resolve.t` asserts that - but nothing says the
address is gone. The group walk does not close this either: a departed peer is
no longer in the list, so it gets no `server_up` and its stale counters stand
alone.

**Key lengths, for the record.** The wrapper copies names into 255-byte
buffers. A peer address longer than that is skipped; in practice nothing
reaches it, since `addr:port` and even a unix socket path at the platform's
`sun_path` limit are far shorter, and `011.long_names.t` holds that. An
oversized `server_name` is different: it is counted under the default zone
rather than dropped, which `011` also holds. Losing the request outright would
be worse - a counter that silently stops is harder to notice than one that
reads high.
