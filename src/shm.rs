//! Shared-memory backing store for VTS counters.
//!
//! Every worker process records into the *same* counters by writing to a
//! fixed-layout `#[repr(C)]` table allocated once from the slab pool
//! attached to the `vts_zone` shared memory.  Lookups are linear scans
//! over a bounded slot array which keeps the layout trivially
//! `#[repr(C)]`-friendly, cap-bounded (no DoS via unbounded zone names),
//! and easy to reason about from raw shared memory.
//!
//! When no `vts_zone` is configured (e.g. during unit tests, or when the
//! user just hasn't declared one yet) the higher-level FFI transparently
//! falls back to the process-local `VTS_MANAGER` defined in `lib.rs`.

use ngx::ffi::*;
use std::collections::HashMap;
use std::os::raw::c_void;
use std::sync::atomic::{AtomicPtr, Ordering};

use crate::stats::{VtsRequestTimes, VtsResponseStats, VtsServerStats};
use crate::upstream_stats::{UpstreamServerStats, UpstreamZone, VtsResponseStats as UpstreamResp};

/// Maximum bytes stored inline for a name/address.  Anything longer is
/// truncated so every slot keeps a plain `[u8; N]` layout.
pub const VTS_NAME_LEN: usize = 128;

/// Number of distinct server-zone keys tracked in the shared table.
pub const VTS_MAX_SERVER_ZONES: usize = 256;

/// Number of distinct (upstream, server) pairs tracked in the shared table.
pub const VTS_MAX_UPSTREAM_SERVERS: usize = 512;

/// Sentinel placed in `request_time_min` for a slot that has never
/// recorded a request.  Any real measurement compares less than this.
const TIME_MIN_UNSET: u64 = u64::MAX;

/// Per server-zone counters laid out directly in shared memory.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VtsServerSlot {
    pub used: u32,
    pub name_len: u32,
    pub name: [u8; VTS_NAME_LEN],
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

/// Per (upstream, server) counters laid out directly in shared memory.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct VtsUpstreamSlot {
    pub used: u32,
    pub upstream_len: u32,
    pub server_len: u32,
    pub _pad: u32,
    pub upstream: [u8; VTS_NAME_LEN],
    pub server: [u8; VTS_NAME_LEN],
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

/// The complete VTS aggregation table.  One instance lives in shared
/// memory for the lifetime of the master process and survives reload via
/// the `data` pointer plumbed through `ngx_init_cycle`.
#[repr(C)]
pub struct VtsSharedTable {
    pub server_zones: [VtsServerSlot; VTS_MAX_SERVER_ZONES],
    pub upstream_servers: [VtsUpstreamSlot; VTS_MAX_UPSTREAM_SERVERS],
}

/// Header stored at `shm_zone->data` so workers can find the table.
#[repr(C)]
pub struct VtsSharedContext {
    pub table: *mut VtsSharedTable,
    pub shpool: *mut ngx_slab_pool_t,
}

/// Globally cached pointers to the live shared table and its slab pool.
/// `vts_init_shm_zone` publishes them once in the master before workers
/// fork; every worker then observes the same shared mapping through
/// these pointers.
static VTS_SHM_TABLE: AtomicPtr<VtsSharedTable> = AtomicPtr::new(std::ptr::null_mut());
static VTS_SHM_POOL: AtomicPtr<ngx_slab_pool_t> = AtomicPtr::new(std::ptr::null_mut());

fn copy_into(dst: &mut [u8; VTS_NAME_LEN], src: &[u8]) -> usize {
    let n = src.len().min(VTS_NAME_LEN);
    dst[..n].copy_from_slice(&src[..n]);
    for b in &mut dst[n..] {
        *b = 0;
    }
    n
}

impl VtsServerSlot {
    fn name_matches(&self, key: &[u8]) -> bool {
        if self.used != 1 || self.name_len as usize != key.len() {
            return false;
        }
        let n = key.len().min(VTS_NAME_LEN);
        self.name[..n] == key[..n]
    }
}

impl VtsUpstreamSlot {
    fn key_matches(&self, upstream: &[u8], server: &[u8]) -> bool {
        if self.used != 1
            || self.upstream_len as usize != upstream.len()
            || self.server_len as usize != server.len()
        {
            return false;
        }
        let un = upstream.len().min(VTS_NAME_LEN);
        let sn = server.len().min(VTS_NAME_LEN);
        self.upstream[..un] == upstream[..un] && self.server[..sn] == server[..sn]
    }
}

impl VtsSharedTable {
    /// Heap-allocate a zeroed table.  Only used by unit tests; the
    /// production path obtains its instance from `ngx_slab_alloc`
    /// followed by `write_bytes(.., 0, ..)` which produces an identical
    /// layout.
    #[cfg(test)]
    pub fn boxed_zeroed() -> Box<Self> {
        // All fields are integers or `u8` arrays, so an all-zero pattern
        // is a valid initialised value of `Self`.
        unsafe {
            let layout = std::alloc::Layout::new::<Self>();
            let ptr = std::alloc::alloc_zeroed(layout) as *mut Self;
            if ptr.is_null() {
                std::alloc::handle_alloc_error(layout);
            }
            Box::from_raw(ptr)
        }
    }

    /// Record a server-zone request.  Silently drops the sample when the
    /// table is full (bounded behaviour, no DoS via the server-zone key).
    pub fn record_server(
        &mut self,
        name: &str,
        status: u16,
        bytes_in: u64,
        bytes_out: u64,
        request_time: u64,
    ) {
        let key_full = name.as_bytes();
        let key = &key_full[..key_full.len().min(VTS_NAME_LEN)];

        let mut free: Option<usize> = None;
        for i in 0..VTS_MAX_SERVER_ZONES {
            let slot = &self.server_zones[i];
            if slot.name_matches(key) {
                self.update_server_slot(i, status, bytes_in, bytes_out, request_time);
                return;
            }
            if slot.used == 0 && free.is_none() {
                free = Some(i);
            }
        }

        if let Some(i) = free {
            {
                let slot = &mut self.server_zones[i];
                slot.used = 1;
                slot.name_len = copy_into(&mut slot.name, key) as u32;
                slot.request_time_min = TIME_MIN_UNSET;
            }
            self.update_server_slot(i, status, bytes_in, bytes_out, request_time);
        }
    }

    fn update_server_slot(
        &mut self,
        i: usize,
        status: u16,
        bytes_in: u64,
        bytes_out: u64,
        request_time: u64,
    ) {
        let slot = &mut self.server_zones[i];
        slot.requests += 1;
        slot.bytes_in += bytes_in;
        slot.bytes_out += bytes_out;
        slot.request_time_total += request_time;
        if request_time > slot.request_time_max {
            slot.request_time_max = request_time;
        }
        if request_time < slot.request_time_min {
            slot.request_time_min = request_time;
        }
        match status {
            100..=199 => slot.status_1xx += 1,
            200..=299 => slot.status_2xx += 1,
            300..=399 => slot.status_3xx += 1,
            400..=499 => slot.status_4xx += 1,
            500..=599 => slot.status_5xx += 1,
            _ => {}
        }
    }

    /// Record an upstream-server request.  Silently drops the sample
    /// when the table is full.
    #[allow(clippy::too_many_arguments)]
    pub fn record_upstream(
        &mut self,
        upstream: &str,
        server: &str,
        request_time: u64,
        upstream_response_time: u64,
        bytes_sent: u64,
        bytes_received: u64,
        status: u16,
    ) {
        let u_full = upstream.as_bytes();
        let s_full = server.as_bytes();
        let u = &u_full[..u_full.len().min(VTS_NAME_LEN)];
        let s = &s_full[..s_full.len().min(VTS_NAME_LEN)];

        let mut free: Option<usize> = None;
        for i in 0..VTS_MAX_UPSTREAM_SERVERS {
            let slot = &self.upstream_servers[i];
            if slot.key_matches(u, s) {
                self.update_upstream_slot(
                    i,
                    request_time,
                    upstream_response_time,
                    bytes_sent,
                    bytes_received,
                    status,
                );
                return;
            }
            if slot.used == 0 && free.is_none() {
                free = Some(i);
            }
        }

        if let Some(i) = free {
            {
                let slot = &mut self.upstream_servers[i];
                slot.used = 1;
                slot.upstream_len = copy_into(&mut slot.upstream, u) as u32;
                slot.server_len = copy_into(&mut slot.server, s) as u32;
            }
            self.update_upstream_slot(
                i,
                request_time,
                upstream_response_time,
                bytes_sent,
                bytes_received,
                status,
            );
        }
    }

    fn update_upstream_slot(
        &mut self,
        i: usize,
        request_time: u64,
        upstream_response_time: u64,
        bytes_sent: u64,
        bytes_received: u64,
        status: u16,
    ) {
        let slot = &mut self.upstream_servers[i];
        slot.request_counter += 1;
        slot.in_bytes += bytes_received;
        slot.out_bytes += bytes_sent;
        if request_time > 0 {
            slot.request_time_total += request_time;
            slot.request_time_counter += 1;
        }
        if upstream_response_time > 0 {
            slot.response_time_total += upstream_response_time;
            slot.response_time_counter += 1;
        }
        match status {
            100..=199 => slot.status_1xx += 1,
            200..=299 => slot.status_2xx += 1,
            300..=399 => slot.status_3xx += 1,
            400..=499 => slot.status_4xx += 1,
            500..=599 => slot.status_5xx += 1,
            _ => {}
        }
    }

    /// Materialize all server-zone slots into the format the Prometheus
    /// formatter expects.
    pub fn snapshot_servers(&self) -> HashMap<String, VtsServerStats> {
        let mut out = HashMap::new();
        for slot in &self.server_zones {
            if slot.used != 1 {
                continue;
            }
            let n = (slot.name_len as usize).min(VTS_NAME_LEN);
            let name = match std::str::from_utf8(&slot.name[..n]) {
                Ok(s) => s.to_string(),
                Err(_) => continue,
            };
            let total = slot.request_time_total as f64 / 1000.0;
            let avg = if slot.requests > 0 {
                total / slot.requests as f64
            } else {
                0.0
            };
            let min = if slot.request_time_min == TIME_MIN_UNSET {
                0.0
            } else {
                slot.request_time_min as f64 / 1000.0
            };
            out.insert(
                name,
                VtsServerStats {
                    requests: slot.requests,
                    bytes_in: slot.bytes_in,
                    bytes_out: slot.bytes_out,
                    responses: VtsResponseStats {
                        status_1xx: slot.status_1xx,
                        status_2xx: slot.status_2xx,
                        status_3xx: slot.status_3xx,
                        status_4xx: slot.status_4xx,
                        status_5xx: slot.status_5xx,
                    },
                    request_times: VtsRequestTimes {
                        total,
                        min,
                        max: slot.request_time_max as f64 / 1000.0,
                        avg,
                    },
                    last_updated: 0,
                },
            );
        }
        out
    }

    /// Materialize upstream slots grouped by upstream name.
    pub fn snapshot_upstreams(&self) -> HashMap<String, UpstreamZone> {
        let mut out: HashMap<String, UpstreamZone> = HashMap::new();
        for slot in &self.upstream_servers {
            if slot.used != 1 {
                continue;
            }
            let un = (slot.upstream_len as usize).min(VTS_NAME_LEN);
            let sn = (slot.server_len as usize).min(VTS_NAME_LEN);
            let upstream = match std::str::from_utf8(&slot.upstream[..un]) {
                Ok(s) => s.to_string(),
                Err(_) => continue,
            };
            let server = match std::str::from_utf8(&slot.server[..sn]) {
                Ok(s) => s.to_string(),
                Err(_) => continue,
            };

            let zone = out
                .entry(upstream.clone())
                .or_insert_with(|| UpstreamZone::new(&upstream));

            let mut stats = UpstreamServerStats::new(&server);
            stats.request_counter = slot.request_counter;
            stats.in_bytes = slot.in_bytes;
            stats.out_bytes = slot.out_bytes;
            stats.responses = UpstreamResp {
                status_1xx: slot.status_1xx,
                status_2xx: slot.status_2xx,
                status_3xx: slot.status_3xx,
                status_4xx: slot.status_4xx,
                status_5xx: slot.status_5xx,
            };
            stats.request_time_total = slot.request_time_total;
            stats.request_time_counter = slot.request_time_counter;
            stats.response_time_total = slot.response_time_total;
            stats.response_time_counter = slot.response_time_counter;
            zone.servers.insert(server, stats);
        }
        out
    }
}

/// Run `f` against the live shared table under the slab pool mutex.
/// Returns `None` when no `vts_zone` is configured.
///
/// The closure must not panic or call back into nginx slab functions;
/// otherwise the shared mutex would be left locked, deadlocking every
/// worker.  Our callers only do plain integer / array work, which
/// satisfies that requirement.
fn with_table<R>(f: impl FnOnce(&mut VtsSharedTable) -> R) -> Option<R> {
    let table = VTS_SHM_TABLE.load(Ordering::Acquire);
    let pool = VTS_SHM_POOL.load(Ordering::Acquire);
    if table.is_null() || pool.is_null() {
        return None;
    }
    unsafe {
        #[cfg(not(test))]
        ngx_shmtx_lock(&mut (*pool).mutex);
        let r = f(&mut *table);
        #[cfg(not(test))]
        ngx_shmtx_unlock(&mut (*pool).mutex);
        Some(r)
    }
}

/// True when a shared zone has been configured and recording will write
/// into it.
#[allow(dead_code)]
pub fn is_configured() -> bool {
    !VTS_SHM_TABLE.load(Ordering::Acquire).is_null()
}

/// Record one server-zone request into shared memory.  Returns false if
/// no zone is configured so the caller can fall back to a process-local
/// store.
pub fn record_server(
    name: &str,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    request_time: u64,
) -> bool {
    with_table(|t| t.record_server(name, status, bytes_in, bytes_out, request_time)).is_some()
}

/// Record one upstream request into shared memory.  Returns false if no
/// zone is configured.
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
    with_table(|t| {
        t.record_upstream(
            upstream,
            server,
            request_time,
            upstream_response_time,
            bytes_sent,
            bytes_received,
            status,
        )
    })
    .is_some()
}

pub fn snapshot_servers() -> Option<HashMap<String, VtsServerStats>> {
    with_table(|t| t.snapshot_servers())
}

pub fn snapshot_upstreams() -> Option<HashMap<String, UpstreamZone>> {
    with_table(|t| t.snapshot_upstreams())
}

/// Shared-memory zone initialization callback.
///
/// Called by nginx exactly once per cycle (in the master, before workers
/// fork).  On reload `data` carries the previous cycle's
/// `shm_zone->data`, so the existing table — and therefore all
/// accumulated statistics — is reused.
///
/// # Safety
///
/// Invoked by nginx via the function pointer stored in `shm_zone->init`.
/// `shm_zone` is a valid pointer; `data` is either NULL on initial start
/// or a `VtsSharedContext*` carried over from the previous cycle.
#[no_mangle]
pub unsafe extern "C" fn vts_init_shm_zone(
    shm_zone: *mut ngx_shm_zone_t,
    data: *mut c_void,
) -> ngx_int_t {
    if shm_zone.is_null() {
        return NGX_ERROR as ngx_int_t;
    }
    let shpool = (*shm_zone).shm.addr as *mut ngx_slab_pool_t;

    // Reload path: inherit the table from the previous cycle.
    let old_ctx = data as *mut VtsSharedContext;
    if !old_ctx.is_null() {
        (*shm_zone).data = data;
        VTS_SHM_TABLE.store((*old_ctx).table, Ordering::Release);
        VTS_SHM_POOL.store(shpool, Ordering::Release);
        return NGX_OK as ngx_int_t;
    }

    // Pre-existing shared mapping (e.g. inherited across binary upgrade):
    // re-discover the context that the slab pool's `data` already points
    // at.
    if (*shm_zone).shm.exists != 0 {
        let ctx = (*shpool).data as *mut VtsSharedContext;
        (*shm_zone).data = ctx as *mut c_void;
        if !ctx.is_null() {
            VTS_SHM_TABLE.store((*ctx).table, Ordering::Release);
            VTS_SHM_POOL.store(shpool, Ordering::Release);
        }
        return NGX_OK as ngx_int_t;
    }

    // First time: allocate the context header and the table from slab,
    // zero the table, then publish.
    let ctx =
        ngx_slab_alloc(shpool, std::mem::size_of::<VtsSharedContext>()) as *mut VtsSharedContext;
    if ctx.is_null() {
        return NGX_ERROR as ngx_int_t;
    }
    let table =
        ngx_slab_alloc(shpool, std::mem::size_of::<VtsSharedTable>()) as *mut VtsSharedTable;
    if table.is_null() {
        return NGX_ERROR as ngx_int_t;
    }
    std::ptr::write_bytes(table as *mut u8, 0, std::mem::size_of::<VtsSharedTable>());

    (*ctx).table = table;
    (*ctx).shpool = shpool;
    (*shm_zone).data = ctx as *mut c_void;
    (*shpool).data = ctx as *mut c_void;

    VTS_SHM_TABLE.store(table, Ordering::Release);
    VTS_SHM_POOL.store(shpool, Ordering::Release);

    NGX_OK as ngx_int_t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_slot_record_and_snapshot() {
        let mut t = VtsSharedTable::boxed_zeroed();

        t.record_server("example.com", 200, 100, 1000, 50);
        t.record_server("example.com", 200, 200, 2000, 70);
        t.record_server("example.com", 404, 50, 80, 10);
        t.record_server("api.example.com", 200, 300, 4000, 30);

        let snap = t.snapshot_servers();
        assert_eq!(snap.len(), 2);

        let s = snap.get("example.com").unwrap();
        assert_eq!(s.requests, 3);
        assert_eq!(s.bytes_in, 350);
        assert_eq!(s.bytes_out, 3080);
        assert_eq!(s.responses.status_2xx, 2);
        assert_eq!(s.responses.status_4xx, 1);
        assert_eq!(s.request_times.max, 0.070); // 70 ms
        assert_eq!(s.request_times.min, 0.010); // 10 ms
        assert!((s.request_times.avg - (130.0 / 3.0 / 1000.0)).abs() < 1e-9);

        let api = snap.get("api.example.com").unwrap();
        assert_eq!(api.requests, 1);
        assert_eq!(api.bytes_in, 300);
        assert_eq!(api.bytes_out, 4000);
    }

    #[test]
    fn upstream_slot_record_and_snapshot() {
        let mut t = VtsSharedTable::boxed_zeroed();

        t.record_upstream("backend", "10.0.0.1:80", 100, 50, 1000, 500, 200);
        t.record_upstream("backend", "10.0.0.1:80", 120, 60, 1200, 600, 200);
        t.record_upstream("backend", "10.0.0.1:80", 150, 70, 1500, 700, 404);
        t.record_upstream("backend", "10.0.0.2:80", 200, 80, 2000, 800, 500);
        t.record_upstream("api", "10.0.1.1:8080", 80, 40, 800, 400, 200);

        let snap = t.snapshot_upstreams();
        assert_eq!(snap.len(), 2);

        let backend = snap.get("backend").unwrap();
        assert_eq!(backend.servers.len(), 2);

        let s1 = backend.servers.get("10.0.0.1:80").unwrap();
        assert_eq!(s1.request_counter, 3);
        assert_eq!(s1.in_bytes, 1800); // 500 + 600 + 700
        assert_eq!(s1.out_bytes, 3700); // 1000 + 1200 + 1500
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
    fn name_longer_than_buffer_is_truncated_consistently() {
        let mut t = VtsSharedTable::boxed_zeroed();
        let long = "z".repeat(VTS_NAME_LEN + 50);

        t.record_server(&long, 200, 1, 1, 5);
        t.record_server(&long, 200, 2, 2, 6);

        let snap = t.snapshot_servers();
        assert_eq!(snap.len(), 1);
        // The stored key is the truncated prefix.
        let trunc = "z".repeat(VTS_NAME_LEN);
        assert_eq!(snap.get(&trunc).unwrap().requests, 2);
    }

    #[test]
    fn table_full_drops_new_keys_but_keeps_updating_existing() {
        let mut t = VtsSharedTable::boxed_zeroed();
        for i in 0..VTS_MAX_SERVER_ZONES {
            t.record_server(&format!("z{i}"), 200, 0, 0, 1);
        }
        // Existing slot still updates.
        t.record_server("z0", 200, 1, 2, 3);
        // New key is dropped silently — table is full.
        t.record_server("overflow", 200, 99, 99, 99);

        let snap = t.snapshot_servers();
        assert_eq!(snap.len(), VTS_MAX_SERVER_ZONES);
        assert_eq!(snap.get("z0").unwrap().requests, 2);
        assert!(!snap.contains_key("overflow"));
    }

    #[test]
    fn no_shm_configured_returns_none() {
        // In a freshly-built test binary the global pointers start null,
        // so higher-level callers fall back to VTS_MANAGER.
        assert!(!is_configured());
        assert!(snapshot_servers().is_none());
        assert!(snapshot_upstreams().is_none());
    }
}
