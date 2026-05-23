//! Shared-memory backing store for VTS counters.
//!
//! The aggregation state is two `RbTreeMap`s — one keyed by `server_name`,
//! one keyed by `"upstream\0server"` — allocated inside the nginx slab
//! pool attached to the `vts_zone` directive.  Capacity scales with the
//! configured zone size: a larger `vts_zone` holds proportionally more
//! distinct keys.
//!
//! Keys come from nginx configuration (the matched server block's first
//! `server_name`, the upstream name from config) — never from the raw
//! `Host` header — so attacker-controlled values cannot expand the key
//! space.
//!
//! When no `vts_zone` is configured (e.g. during unit tests, or when the
//! user just hasn't declared one yet) the higher-level FFI transparently
//! falls back to the process-local `VTS_MANAGER` defined in `lib.rs`.

#[cfg(not(test))]
use ngx::allocator::allocate;
use ngx::collections::RbTreeMap;
use ngx::core::{NgxString, SlabPool};
use ngx::ffi::*;
use ngx::sync::RwLock;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::cache_stats::{CacheZoneStats, VtsCacheStats};
use crate::stats::{VtsRequestTimes, VtsResponseStats, VtsServerStats};
use crate::upstream_stats::{UpstreamServerStats, UpstreamZone, VtsResponseStats as UpstreamResp};

/// Sanity upper bound on the byte length of a single key.  The matched
/// `server_name` and upstream/server values come from nginx config rather
/// than attacker input, so this is a defensive cap against misconfiguration
/// (typo, accidental huge value) rather than a security boundary.
#[cfg_attr(test, allow(dead_code))]
pub const VTS_MAX_KEY_BYTES: usize = 256;

/// Sentinel placed in `request_time_min` for an entry that has never
/// recorded a request.  Any real measurement compares less than this.
const TIME_MIN_UNSET: u64 = u64::MAX;

/// Per server-zone counters stored as the value in the `servers` map.
#[derive(Clone, Copy)]
pub struct ServerCounters {
    pub requests: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub status_1xx: u64,
    pub status_2xx: u64,
    pub status_3xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    pub request_time_total: u64,
    pub request_time_max: u64,
    pub request_time_min: u64,
}

impl ServerCounters {
    fn new() -> Self {
        Self {
            requests: 0,
            bytes_in: 0,
            bytes_out: 0,
            status_1xx: 0,
            status_2xx: 0,
            status_3xx: 0,
            status_4xx: 0,
            status_5xx: 0,
            request_time_total: 0,
            request_time_max: 0,
            request_time_min: TIME_MIN_UNSET,
        }
    }

    /// Convert into the output-side struct that the Prometheus formatter
    /// consumes.
    fn into_stats(self) -> VtsServerStats {
        let total = self.request_time_total as f64 / 1000.0;
        let avg = if self.requests > 0 {
            total / self.requests as f64
        } else {
            0.0
        };
        let min = if self.request_time_min == TIME_MIN_UNSET {
            0.0
        } else {
            self.request_time_min as f64 / 1000.0
        };
        VtsServerStats {
            requests: self.requests,
            bytes_in: self.bytes_in,
            bytes_out: self.bytes_out,
            responses: VtsResponseStats {
                status_1xx: self.status_1xx,
                status_2xx: self.status_2xx,
                status_3xx: self.status_3xx,
                status_4xx: self.status_4xx,
                status_5xx: self.status_5xx,
            },
            request_times: VtsRequestTimes {
                total,
                min,
                max: self.request_time_max as f64 / 1000.0,
                avg,
            },
            last_updated: 0,
        }
    }

    fn update(&mut self, status: u16, bytes_in: u64, bytes_out: u64, request_time: u64) {
        self.requests += 1;
        self.bytes_in += bytes_in;
        self.bytes_out += bytes_out;
        self.request_time_total += request_time;
        if request_time > self.request_time_max {
            self.request_time_max = request_time;
        }
        if request_time < self.request_time_min {
            self.request_time_min = request_time;
        }
        match status {
            100..=199 => self.status_1xx += 1,
            200..=299 => self.status_2xx += 1,
            300..=399 => self.status_3xx += 1,
            400..=499 => self.status_4xx += 1,
            500..=599 => self.status_5xx += 1,
            _ => {}
        }
    }
}

/// Per (upstream, server) counters stored as the value in the
/// `upstreams` map.
#[derive(Clone, Copy)]
pub struct UpstreamCounters {
    pub request_counter: u64,
    pub in_bytes: u64,
    pub out_bytes: u64,
    pub status_1xx: u64,
    pub status_2xx: u64,
    pub status_3xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    pub request_time_total: u64,
    pub request_time_counter: u64,
    pub response_time_total: u64,
    pub response_time_counter: u64,
}

impl UpstreamCounters {
    fn new() -> Self {
        Self {
            request_counter: 0,
            in_bytes: 0,
            out_bytes: 0,
            status_1xx: 0,
            status_2xx: 0,
            status_3xx: 0,
            status_4xx: 0,
            status_5xx: 0,
            request_time_total: 0,
            request_time_counter: 0,
            response_time_total: 0,
            response_time_counter: 0,
        }
    }

    /// Populate the output-side `UpstreamServerStats` consumed by the
    /// Prometheus formatter.
    fn into_stats(self, server: &str) -> UpstreamServerStats {
        let mut stats = UpstreamServerStats::new(server);
        stats.request_counter = self.request_counter;
        stats.in_bytes = self.in_bytes;
        stats.out_bytes = self.out_bytes;
        stats.responses = UpstreamResp {
            status_1xx: self.status_1xx,
            status_2xx: self.status_2xx,
            status_3xx: self.status_3xx,
            status_4xx: self.status_4xx,
            status_5xx: self.status_5xx,
        };
        stats.request_time_total = self.request_time_total;
        stats.request_time_counter = self.request_time_counter;
        stats.response_time_total = self.response_time_total;
        stats.response_time_counter = self.response_time_counter;
        stats
    }

    fn update(
        &mut self,
        request_time: u64,
        upstream_response_time: u64,
        bytes_sent: u64,
        bytes_received: u64,
        status: u16,
    ) {
        self.request_counter += 1;
        self.in_bytes += bytes_received;
        self.out_bytes += bytes_sent;
        if request_time > 0 {
            self.request_time_total += request_time;
            self.request_time_counter += 1;
        }
        if upstream_response_time > 0 {
            self.response_time_total += upstream_response_time;
            self.response_time_counter += 1;
        }
        match status {
            100..=199 => self.status_1xx += 1,
            200..=299 => self.status_2xx += 1,
            300..=399 => self.status_3xx += 1,
            400..=499 => self.status_4xx += 1,
            500..=599 => self.status_5xx += 1,
            _ => {}
        }
    }
}

/// Per cache-zone counters stored as the value in the `caches` map.
///
/// Mirrors the variants nginx exposes via `$upstream_cache_status`
/// (1=MISS .. 8=SCARCE); a value of 0 means "no cache" and is filtered
/// at the call site rather than counted here.
#[derive(Clone, Copy)]
pub struct CacheCounters {
    pub miss: u64,
    pub bypass: u64,
    pub expired: u64,
    pub stale: u64,
    pub updating: u64,
    pub revalidated: u64,
    pub hit: u64,
    pub scarce: u64,
}

impl CacheCounters {
    fn new() -> Self {
        Self {
            miss: 0,
            bypass: 0,
            expired: 0,
            stale: 0,
            updating: 0,
            revalidated: 0,
            hit: 0,
            scarce: 0,
        }
    }

    /// Apply one cache-status observation.  `status` is the raw
    /// `ngx_uint_t` from `r->upstream->cache_status` (the same numeric
    /// scheme `$upstream_cache_status` is derived from).  Unknown
    /// values are ignored.
    fn update(&mut self, status: u8) {
        match status {
            1 => self.miss += 1,
            2 => self.bypass += 1,
            3 => self.expired += 1,
            4 => self.stale += 1,
            5 => self.updating += 1,
            6 => self.revalidated += 1,
            7 => self.hit += 1,
            8 => self.scarce += 1,
            _ => {}
        }
    }

    /// Convert into the output-side struct that the Prometheus formatter
    /// consumes.
    fn into_stats(self, zone: &str) -> CacheZoneStats {
        let mut out = CacheZoneStats::new(zone);
        out.cache = VtsCacheStats {
            miss: self.miss,
            bypass: self.bypass,
            expired: self.expired,
            stale: self.stale,
            updating: self.updating,
            revalidated: self.revalidated,
            hit: self.hit,
            scarce: self.scarce,
        };
        out
    }
}

/// Format the upstream map key as `"upstream\0server"`.  Keys come from
/// nginx configuration and never contain a NUL byte, so the separator is
/// unambiguous.
fn upstream_key_bytes(upstream: &str, server: &str) -> Vec<u8> {
    let mut v = Vec::with_capacity(upstream.len() + 1 + server.len());
    v.extend_from_slice(upstream.as_bytes());
    v.push(0);
    v.extend_from_slice(server.as_bytes());
    v
}

/// Inverse of `upstream_key_bytes`: split on the first NUL byte.
fn split_upstream_key(bytes: &[u8]) -> Option<(&[u8], &[u8])> {
    let nul = bytes.iter().position(|&b| b == 0)?;
    Some((&bytes[..nul], &bytes[nul + 1..]))
}

/// `RbTreeMap` keyed by server-zone name, stored in the slab pool.
pub type ServerMap<A> = RbTreeMap<NgxString<A>, ServerCounters, A>;

/// `RbTreeMap` keyed by the upstream/server byte composite, stored in the
/// slab pool.
pub type UpstreamMap<A> = RbTreeMap<NgxString<A>, UpstreamCounters, A>;

/// `RbTreeMap` keyed by cache-zone name, stored in the slab pool.
pub type CacheMap<A> = RbTreeMap<NgxString<A>, CacheCounters, A>;

/// Root of the shared-memory state, allocated once from the slab pool.
#[cfg_attr(test, allow(dead_code))]
pub struct VtsShared {
    pub servers: RwLock<ServerMap<SlabPool>>,
    pub upstreams: RwLock<UpstreamMap<SlabPool>>,
    pub caches: RwLock<CacheMap<SlabPool>>,
}

/// Pointer published once by `vts_init_shm_zone` (in the master, before
/// workers fork) and observed by every worker thereafter.  Null until a
/// `vts_zone` is configured, in which case the higher-level FFI falls
/// back to the process-local manager.
static VTS_SHARED: AtomicPtr<VtsShared> = AtomicPtr::new(std::ptr::null_mut());

/// True when a shared zone has been configured and recording will write
/// into it.
#[allow(dead_code)]
pub fn is_configured() -> bool {
    !VTS_SHARED.load(Ordering::Acquire).is_null()
}

#[cfg(not(test))]
fn shared() -> Option<&'static VtsShared> {
    let ptr = VTS_SHARED.load(Ordering::Acquire);
    if ptr.is_null() {
        None
    } else {
        // SAFETY: the pointer is published once before workers fork and
        // remains valid for the lifetime of the master/worker processes.
        Some(unsafe { &*ptr })
    }
}

/// Record one server-zone request into shared memory.  Returns `false`
/// when no `vts_zone` is configured so the caller can fall back to a
/// process-local store.  Oversized keys (> `VTS_MAX_KEY_BYTES`) and
/// out-of-memory inserts are silently dropped while reporting `true`
/// (the shared path *is* configured; we just don't have room for this
/// specific key).
#[cfg(not(test))]
pub fn record_server(
    name: &str,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    request_time: u64,
) -> bool {
    let Some(shared) = shared() else {
        return false;
    };
    if name.is_empty() || name.len() > VTS_MAX_KEY_BYTES {
        return true;
    }

    let key_bytes = name.as_bytes();
    let mut guard = shared.servers.write();

    if let Some(entry) = guard.get_mut(key_bytes) {
        entry.update(status, bytes_in, bytes_out, request_time);
        return true;
    }

    let alloc = guard.allocator().clone();
    let Ok(key) = NgxString::try_from_bytes_in(key_bytes, alloc) else {
        return true;
    };
    let mut counters = ServerCounters::new();
    counters.update(status, bytes_in, bytes_out, request_time);
    let _ = guard.try_insert(key, counters);
    true
}

/// Test-only stub: pretends no `vts_zone` is configured so callers fall
/// back to the process-local manager.  Avoids linking the slab allocator
/// and `ngx::sync::RwLock` into the unit-test binary.
#[cfg(test)]
pub fn record_server(
    _name: &str,
    _status: u16,
    _bytes_in: u64,
    _bytes_out: u64,
    _request_time: u64,
) -> bool {
    false
}

/// Record one upstream-server request into shared memory.  See
/// [`record_server`] for the return-value contract.
#[cfg(not(test))]
#[allow(clippy::too_many_arguments)]
pub fn record_upstream(
    upstream: &str,
    server: &str,
    request_time: u64,
    upstream_response_time: u64,
    bytes_sent: u64,
    bytes_received: u64,
    status: u16,
) -> bool {
    let Some(shared) = shared() else {
        return false;
    };
    if upstream.is_empty()
        || server.is_empty()
        || upstream.len() > VTS_MAX_KEY_BYTES
        || server.len() > VTS_MAX_KEY_BYTES
    {
        return true;
    }

    let composite = upstream_key_bytes(upstream, server);
    let mut guard = shared.upstreams.write();

    if let Some(entry) = guard.get_mut(composite.as_slice()) {
        entry.update(
            request_time,
            upstream_response_time,
            bytes_sent,
            bytes_received,
            status,
        );
        return true;
    }

    let alloc = guard.allocator().clone();
    let Ok(key) = NgxString::try_from_bytes_in(&composite, alloc) else {
        return true;
    };
    let mut counters = UpstreamCounters::new();
    counters.update(
        request_time,
        upstream_response_time,
        bytes_sent,
        bytes_received,
        status,
    );
    let _ = guard.try_insert(key, counters);
    true
}

/// Test-only stub.  See [`record_server`].
#[cfg(test)]
#[allow(clippy::too_many_arguments)]
pub fn record_upstream(
    _upstream: &str,
    _server: &str,
    _request_time: u64,
    _upstream_response_time: u64,
    _bytes_sent: u64,
    _bytes_received: u64,
    _status: u16,
) -> bool {
    false
}

/// Record one cache-status observation into shared memory.  Returns
/// `false` when no `vts_zone` is configured so the caller can fall back
/// to the process-local `CACHE_MANAGER`.  Oversized zone names and
/// out-of-memory inserts are silently dropped while reporting `true`.
#[cfg(not(test))]
pub fn record_cache(zone: &str, status: u8) -> bool {
    let Some(shared) = shared() else {
        return false;
    };
    if zone.is_empty() || zone.len() > VTS_MAX_KEY_BYTES {
        return true;
    }

    let key_bytes = zone.as_bytes();
    let mut guard = shared.caches.write();

    if let Some(entry) = guard.get_mut(key_bytes) {
        entry.update(status);
        return true;
    }

    let alloc = guard.allocator().clone();
    let Ok(key) = NgxString::try_from_bytes_in(key_bytes, alloc) else {
        return true;
    };
    let mut counters = CacheCounters::new();
    counters.update(status);
    let _ = guard.try_insert(key, counters);
    true
}

/// Test-only stub.  See [`record_server`].
#[cfg(test)]
pub fn record_cache(_zone: &str, _status: u8) -> bool {
    false
}

/// Build the Prometheus-side server map from any iterator of
/// `(key_bytes, counters)` pairs.  Used by both the production slab path
/// and the unit tests (with plain heap-allocated maps).
fn build_server_snapshot<'a, I>(entries: I) -> HashMap<String, VtsServerStats>
where
    I: IntoIterator<Item = (&'a [u8], &'a ServerCounters)>,
{
    let mut out = HashMap::new();
    for (key_bytes, counters) in entries {
        if let Ok(name) = std::str::from_utf8(key_bytes) {
            out.insert(name.to_string(), (*counters).into_stats());
        }
    }
    out
}

/// Build the Prometheus-side upstream map from any iterator of
/// `(composite_key_bytes, counters)` pairs.  See [`build_server_snapshot`].
fn build_upstream_snapshot<'a, I>(entries: I) -> HashMap<String, UpstreamZone>
where
    I: IntoIterator<Item = (&'a [u8], &'a UpstreamCounters)>,
{
    let mut out: HashMap<String, UpstreamZone> = HashMap::new();
    for (key_bytes, counters) in entries {
        let Some((upstream_bytes, server_bytes)) = split_upstream_key(key_bytes) else {
            continue;
        };
        let Ok(upstream) = std::str::from_utf8(upstream_bytes) else {
            continue;
        };
        let Ok(server) = std::str::from_utf8(server_bytes) else {
            continue;
        };
        let zone = out
            .entry(upstream.to_string())
            .or_insert_with(|| UpstreamZone::new(upstream));
        zone.servers
            .insert(server.to_string(), (*counters).into_stats(server));
    }
    out
}

/// Build the Prometheus-side cache map from any iterator of
/// `(zone_name_bytes, counters)` pairs.
fn build_cache_snapshot<'a, I>(entries: I) -> HashMap<String, CacheZoneStats>
where
    I: IntoIterator<Item = (&'a [u8], &'a CacheCounters)>,
{
    let mut out = HashMap::new();
    for (key_bytes, counters) in entries {
        if let Ok(zone) = std::str::from_utf8(key_bytes) {
            out.insert(zone.to_string(), (*counters).into_stats(zone));
        }
    }
    out
}

/// Materialize all server-zone counters into the format the Prometheus
/// formatter expects.  Returns `None` when no `vts_zone` is configured.
#[cfg(not(test))]
pub fn snapshot_servers() -> Option<HashMap<String, VtsServerStats>> {
    let shared = shared()?;
    let guard = shared.servers.read();
    Some(build_server_snapshot(
        guard.iter().map(|(k, v)| (k.as_bytes(), v)),
    ))
}

/// Test-only stub.  See [`record_server`].
#[cfg(test)]
pub fn snapshot_servers() -> Option<HashMap<String, VtsServerStats>> {
    None
}

/// Materialize upstream counters grouped by upstream name.  Returns
/// `None` when no `vts_zone` is configured.
#[cfg(not(test))]
pub fn snapshot_upstreams() -> Option<HashMap<String, UpstreamZone>> {
    let shared = shared()?;
    let guard = shared.upstreams.read();
    Some(build_upstream_snapshot(
        guard.iter().map(|(k, v)| (k.as_bytes(), v)),
    ))
}

/// Test-only stub.  See [`record_server`].
#[cfg(test)]
pub fn snapshot_upstreams() -> Option<HashMap<String, UpstreamZone>> {
    None
}

/// Materialize all cache-zone counters into the format the Prometheus
/// formatter expects.  Returns `None` when no `vts_zone` is configured.
#[cfg(not(test))]
pub fn snapshot_caches() -> Option<HashMap<String, CacheZoneStats>> {
    let shared = shared()?;
    let guard = shared.caches.read();
    Some(build_cache_snapshot(
        guard.iter().map(|(k, v)| (k.as_bytes(), v)),
    ))
}

/// Test-only stub.  See [`record_server`].
#[cfg(test)]
pub fn snapshot_caches() -> Option<HashMap<String, CacheZoneStats>> {
    None
}

/// Shared-memory zone initialization callback.
///
/// Called by nginx exactly once per cycle (in the master, before workers
/// fork).  On reload the slab pool's `data` field still points at the
/// previous cycle's `VtsShared`, so we just re-publish that pointer;
/// otherwise we allocate two empty `RbTreeMap`s and a fresh `VtsShared`
/// from the slab pool itself.
///
/// # Safety
///
/// Invoked by nginx via the function pointer stored in `shm_zone->init`.
/// `shm_zone` is a valid pointer; `data` is either NULL on initial start
/// or a pointer carried over from the previous cycle (we re-discover the
/// shared state from the slab pool either way, so we don't need it).
#[cfg(not(test))]
#[no_mangle]
pub unsafe extern "C" fn vts_init_shm_zone(
    shm_zone: *mut ngx_shm_zone_t,
    _data: *mut c_void,
) -> ngx_int_t {
    if shm_zone.is_null() {
        return NGX_ERROR as ngx_int_t;
    }
    let shm_zone_ref = &mut *shm_zone;

    let mut alloc = match SlabPool::from_shm_zone(shm_zone_ref) {
        Some(a) => a,
        None => return NGX_ERROR as ngx_int_t,
    };

    // The slab pool's `data` field persists across reload and binary
    // upgrade because it lives in the shared memory itself.  Non-null
    // means a previous cycle (or this same cycle, on reload) already
    // built the shared state — just re-publish the pointer.
    let existing = alloc.as_mut().data as *mut VtsShared;
    if !existing.is_null() {
        shm_zone_ref.data = existing as *mut c_void;
        VTS_SHARED.store(existing, Ordering::Release);
        return NGX_OK as ngx_int_t;
    }

    // First time: create empty maps inside the slab pool and store them
    // inside a `VtsShared` allocated from the same pool.
    let servers: ServerMap<SlabPool> = match RbTreeMap::try_new_in(alloc.clone()) {
        Ok(m) => m,
        Err(_) => return NGX_ERROR as ngx_int_t,
    };
    let upstreams: UpstreamMap<SlabPool> = match RbTreeMap::try_new_in(alloc.clone()) {
        Ok(m) => m,
        Err(_) => return NGX_ERROR as ngx_int_t,
    };
    let caches: CacheMap<SlabPool> = match RbTreeMap::try_new_in(alloc.clone()) {
        Ok(m) => m,
        Err(_) => return NGX_ERROR as ngx_int_t,
    };
    let shared = VtsShared {
        servers: RwLock::new(servers),
        upstreams: RwLock::new(upstreams),
        caches: RwLock::new(caches),
    };
    let shared_ptr: *mut VtsShared = match allocate(shared, &alloc) {
        Ok(p) => p.as_ptr(),
        Err(_) => return NGX_ERROR as ngx_int_t,
    };

    shm_zone_ref.data = shared_ptr as *mut c_void;
    alloc.as_mut().data = shared_ptr as *mut c_void;
    VTS_SHARED.store(shared_ptr, Ordering::Release);

    NGX_OK as ngx_int_t
}

/// Test-only stub.  Tests never invoke this; provided so the symbol
/// exists if anything else in the build references it.
#[cfg(test)]
#[no_mangle]
pub unsafe extern "C" fn vts_init_shm_zone(
    _shm_zone: *mut ngx_shm_zone_t,
    _data: *mut c_void,
) -> ngx_int_t {
    NGX_ERROR as ngx_int_t
}

#[cfg(test)]
mod tests {
    //! Unit tests exercise the pieces that don't require nginx's C-side
    //! rbtree symbols: counter accumulation, key encoding, and the
    //! snapshot conversion from `(&[u8], &Counters)` pairs into the
    //! Prometheus-side structs.  The full slab-pool + `RbTreeMap` path is
    //! covered by integration tests run against an actual nginx process.

    use super::*;

    #[test]
    fn server_counters_accumulate_correctly() {
        let mut c = ServerCounters::new();
        c.update(200, 100, 1000, 50);
        c.update(200, 200, 2000, 70);
        c.update(404, 50, 80, 10);

        assert_eq!(c.requests, 3);
        assert_eq!(c.bytes_in, 350);
        assert_eq!(c.bytes_out, 3080);
        assert_eq!(c.status_2xx, 2);
        assert_eq!(c.status_4xx, 1);
        assert_eq!(c.request_time_max, 70);
        assert_eq!(c.request_time_min, 10);
        assert_eq!(c.request_time_total, 130);
    }

    #[test]
    fn server_counters_status_buckets_cover_all_classes() {
        let mut c = ServerCounters::new();
        for status in [101u16, 204, 304, 404, 503, 600] {
            c.update(status, 0, 0, 0);
        }
        assert_eq!(c.status_1xx, 1);
        assert_eq!(c.status_2xx, 1);
        assert_eq!(c.status_3xx, 1);
        assert_eq!(c.status_4xx, 1);
        assert_eq!(c.status_5xx, 1);
        // 600 is outside the documented range and is ignored.
        assert_eq!(c.requests, 6);
    }

    #[test]
    fn server_counters_into_stats_handles_unset_min() {
        let c = ServerCounters::new();
        let stats = c.into_stats();
        assert_eq!(stats.requests, 0);
        // Unset sentinel must surface as 0.0, not the f64 of u64::MAX.
        assert_eq!(stats.request_times.min, 0.0);
        assert_eq!(stats.request_times.avg, 0.0);
    }

    #[test]
    fn upstream_counters_accumulate_correctly() {
        let mut c = UpstreamCounters::new();
        c.update(100, 50, 1000, 500, 200);
        c.update(120, 60, 1200, 600, 200);
        c.update(150, 70, 1500, 700, 404);

        assert_eq!(c.request_counter, 3);
        assert_eq!(c.in_bytes, 1800);
        assert_eq!(c.out_bytes, 3700);
        assert_eq!(c.status_2xx, 2);
        assert_eq!(c.status_4xx, 1);
        assert_eq!(c.request_time_total, 370);
        assert_eq!(c.request_time_counter, 3);
        assert_eq!(c.response_time_total, 180);
        assert_eq!(c.response_time_counter, 3);
    }

    #[test]
    fn upstream_counters_skip_zero_timings() {
        let mut c = UpstreamCounters::new();
        c.update(0, 0, 100, 50, 200);
        c.update(50, 0, 100, 50, 200);
        c.update(0, 30, 100, 50, 200);

        assert_eq!(c.request_counter, 3);
        // Only the second call contributes a request_time sample.
        assert_eq!(c.request_time_counter, 1);
        assert_eq!(c.request_time_total, 50);
        // Only the third call contributes an upstream_response_time sample.
        assert_eq!(c.response_time_counter, 1);
        assert_eq!(c.response_time_total, 30);
    }

    #[test]
    fn upstream_key_round_trip() {
        let composite = upstream_key_bytes("backend", "10.0.0.1:80");
        let (u, s) = split_upstream_key(&composite).unwrap();
        assert_eq!(u, b"backend");
        assert_eq!(s, b"10.0.0.1:80");
    }

    #[test]
    fn split_upstream_key_rejects_missing_separator() {
        // No NUL byte — caller should reject.
        assert!(split_upstream_key(b"no-nul-here").is_none());
    }

    #[test]
    fn build_server_snapshot_converts_entries() {
        let mut c1 = ServerCounters::new();
        c1.update(200, 100, 1000, 50);
        c1.update(200, 200, 2000, 70);
        c1.update(404, 50, 80, 10);
        let mut c2 = ServerCounters::new();
        c2.update(200, 300, 4000, 30);

        let entries: Vec<(&[u8], &ServerCounters)> = vec![
            (b"example.com".as_ref(), &c1),
            (b"api.example.com".as_ref(), &c2),
        ];
        let snap = build_server_snapshot(entries);
        assert_eq!(snap.len(), 2);

        let s = snap.get("example.com").unwrap();
        assert_eq!(s.requests, 3);
        assert_eq!(s.bytes_in, 350);
        assert_eq!(s.bytes_out, 3080);
        assert_eq!(s.responses.status_2xx, 2);
        assert_eq!(s.responses.status_4xx, 1);
        assert_eq!(s.request_times.max, 0.070);
        assert_eq!(s.request_times.min, 0.010);
        assert!((s.request_times.avg - (130.0 / 3.0 / 1000.0)).abs() < 1e-9);

        let api = snap.get("api.example.com").unwrap();
        assert_eq!(api.requests, 1);
    }

    #[test]
    fn build_upstream_snapshot_groups_by_upstream_name() {
        let mut c_be1 = UpstreamCounters::new();
        c_be1.update(100, 50, 1000, 500, 200);
        c_be1.update(120, 60, 1200, 600, 200);
        c_be1.update(150, 70, 1500, 700, 404);
        let mut c_be2 = UpstreamCounters::new();
        c_be2.update(200, 80, 2000, 800, 500);
        let mut c_api = UpstreamCounters::new();
        c_api.update(80, 40, 800, 400, 200);

        let k_be1 = upstream_key_bytes("backend", "10.0.0.1:80");
        let k_be2 = upstream_key_bytes("backend", "10.0.0.2:80");
        let k_api = upstream_key_bytes("api", "10.0.1.1:8080");

        let entries: Vec<(&[u8], &UpstreamCounters)> = vec![
            (k_be1.as_slice(), &c_be1),
            (k_be2.as_slice(), &c_be2),
            (k_api.as_slice(), &c_api),
        ];
        let snap = build_upstream_snapshot(entries);
        assert_eq!(snap.len(), 2);

        let backend = snap.get("backend").unwrap();
        assert_eq!(backend.servers.len(), 2);

        let s1 = backend.servers.get("10.0.0.1:80").unwrap();
        assert_eq!(s1.request_counter, 3);
        assert_eq!(s1.in_bytes, 1800);
        assert_eq!(s1.out_bytes, 3700);
        assert_eq!(s1.responses.status_2xx, 2);
        assert_eq!(s1.responses.status_4xx, 1);
        assert_eq!(s1.request_time_total, 370);
        assert_eq!(s1.request_time_counter, 3);

        let s2 = backend.servers.get("10.0.0.2:80").unwrap();
        assert_eq!(s2.request_counter, 1);
        assert_eq!(s2.responses.status_5xx, 1);

        let api = snap.get("api").unwrap();
        assert_eq!(api.servers.len(), 1);
    }

    #[test]
    fn build_upstream_snapshot_skips_malformed_keys() {
        let counters = UpstreamCounters::new();
        let entries: Vec<(&[u8], &UpstreamCounters)> = vec![(b"no-nul".as_ref(), &counters)];
        let snap = build_upstream_snapshot(entries);
        assert!(snap.is_empty());
    }

    #[test]
    fn no_shm_configured_returns_none() {
        // In the test binary the global pointer starts null, so the
        // higher-level callers fall back to VTS_MANAGER.
        assert!(!is_configured());
        assert!(snapshot_servers().is_none());
        assert!(snapshot_upstreams().is_none());
        assert!(snapshot_caches().is_none());
        assert!(!record_server("test", 200, 0, 0, 0));
        assert!(!record_upstream("u", "s", 0, 0, 0, 0, 200));
        assert!(!record_cache("zone", 7));
    }

    #[test]
    fn cache_counters_accumulate_correctly() {
        let mut c = CacheCounters::new();
        // Two HITs, one MISS, one BYPASS, one EXPIRED.
        c.update(7);
        c.update(7);
        c.update(1);
        c.update(2);
        c.update(3);

        assert_eq!(c.hit, 2);
        assert_eq!(c.miss, 1);
        assert_eq!(c.bypass, 1);
        assert_eq!(c.expired, 1);
        assert_eq!(c.stale, 0);
        assert_eq!(c.updating, 0);
        assert_eq!(c.revalidated, 0);
        assert_eq!(c.scarce, 0);
    }

    #[test]
    fn cache_counters_cover_all_variants() {
        let mut c = CacheCounters::new();
        for status in 1u8..=8 {
            c.update(status);
        }
        assert_eq!(c.miss, 1);
        assert_eq!(c.bypass, 1);
        assert_eq!(c.expired, 1);
        assert_eq!(c.stale, 1);
        assert_eq!(c.updating, 1);
        assert_eq!(c.revalidated, 1);
        assert_eq!(c.hit, 1);
        assert_eq!(c.scarce, 1);
    }

    #[test]
    fn cache_counters_ignore_unknown_status() {
        let mut c = CacheCounters::new();
        c.update(0); // "no cache" sentinel
        c.update(9); // out-of-range
        c.update(7); // HIT
        assert_eq!(c.hit, 1);
        // No other counter incremented.
        assert_eq!(
            c.miss + c.bypass + c.expired + c.stale + c.updating + c.revalidated + c.scarce,
            0
        );
    }

    #[test]
    fn cache_counters_into_stats() {
        let mut c = CacheCounters::new();
        c.update(7);
        c.update(7);
        c.update(1);
        c.update(2);

        let stats = c.into_stats("my_cache");
        assert_eq!(stats.name, "my_cache");
        assert_eq!(stats.cache.hit, 2);
        assert_eq!(stats.cache.miss, 1);
        assert_eq!(stats.cache.bypass, 1);
        assert_eq!(stats.cache.total_requests(), 4);
        assert_eq!(stats.cache.hit_ratio(), 50.0);
        // Size remains default — this PR doesn't wire up cache size.
        assert_eq!(stats.size.max_size, 0);
        assert_eq!(stats.size.used_size, 0);
    }

    #[test]
    fn build_cache_snapshot_converts_entries() {
        let mut c1 = CacheCounters::new();
        c1.update(7);
        c1.update(7);
        c1.update(1);
        let mut c2 = CacheCounters::new();
        c2.update(2);
        c2.update(2);

        let entries: Vec<(&[u8], &CacheCounters)> =
            vec![(b"static".as_ref(), &c1), (b"api".as_ref(), &c2)];
        let snap = build_cache_snapshot(entries);
        assert_eq!(snap.len(), 2);

        let s = snap.get("static").unwrap();
        assert_eq!(s.name, "static");
        assert_eq!(s.cache.hit, 2);
        assert_eq!(s.cache.miss, 1);

        let a = snap.get("api").unwrap();
        assert_eq!(a.cache.bypass, 2);
        assert_eq!(a.cache.hit, 0);
    }

    #[test]
    fn build_cache_snapshot_skips_non_utf8_keys() {
        let counters = CacheCounters::new();
        let bad: Vec<(&[u8], &CacheCounters)> = vec![(&[0xFF, 0xFE][..], &counters)];
        let snap = build_cache_snapshot(bad);
        assert!(snap.is_empty());
    }
}
