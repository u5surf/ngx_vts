//! LOG_PHASE request handler.
//!
//! This is the former `src/ngx_vts_wrapper.c`, ported to Rust.  nginx
//! runs this handler once per request after the response has been
//! sent; it reads the request, upstream and cache state straight off
//! `ngx_http_request_t` and feeds the crate's stats tables.
//!
//! The whole module is `cfg(not(test))`: it links against nginx
//! symbols (`ngx_http_core_module`, `ngx_array_push`, …) that don't
//! exist in the unit-test binary.  The arithmetic and recording
//! helpers it calls live in `lib.rs` and are covered there.

use ngx::core::Status;
use ngx::ffi::*;
use ngx::http::{
    add_phase_handler, HttpModuleServerConf, HttpPhase, HttpRequestHandler, NgxHttpCoreModule,
    Request,
};

use crate::shm::VTS_MAX_KEY_BYTES;

/// The zone a request is counted against when its `server_name` cannot
/// be used as a key.
const DEFAULT_SERVER_ZONE: &str = "_";

/// Borrow an `ngx_str_t` as a `&str` usable as a table key.  `None` for
/// empty, NULL-data, non-UTF-8, or over-long strings — every caller
/// here treats those as "no value".
///
/// The length check is the same one the shared table applies in
/// `shm::record_*`: a longer key is refused there, so recognising it
/// here lets the caller decide what to do instead of silently dropping
/// the observation. It also keeps the process-local fallback maps from
/// growing keys the shared table would never accept.
///
/// # Safety
///
/// `s` must point at a valid `ngx_str_t` whose `data`/`len` describe
/// memory that outlives `'a`.
unsafe fn as_key<'a>(s: &ngx_str_t) -> Option<&'a str> {
    if s.len == 0 || s.len > VTS_MAX_KEY_BYTES || s.data.is_null() {
        return None;
    }
    std::str::from_utf8(std::slice::from_raw_parts(s.data, s.len)).ok()
}

/// `off_t` counters are signed; clamp rather than wrap on the
/// (shouldn't-happen) negative case.
fn to_u64(v: off_t) -> u64 {
    v.max(0) as u64
}

pub(crate) struct VtsLogHandler;

impl HttpRequestHandler for VtsLogHandler {
    const PHASE: HttpPhase = HttpPhase::Log;
    type Output = Status;

    fn handler(req: &mut Request) -> Status {
        // Count each user-facing request exactly once.  nginx fires the
        // LOG_PHASE handler for every subrequest as well as the main
        // request (auth_request, addition, SSI, X-Accel-Redirect, …);
        // letting those through would double-count both server-zone and
        // upstream counters.
        if !req.is_main() {
            return Status::NGX_DECLINED;
        }

        let r: *const ngx_http_request_t = (&*req).into();

        // SAFETY: nginx hands us a live request for the duration of the
        // LOG_PHASE call, and every pointer we chase off it is
        // null-checked before use.
        unsafe {
            // Skip Prometheus scrapes: the vts_status content handler
            // sets a non-NULL ctx on the request before rendering,
            // which lets us exclude /status from server_zone counters
            // here.  Otherwise every scrape would inflate
            // `nginx_vts_server_requests_total` for whichever vhost
            // hosts /status.  The slot holds a bare sentinel address,
            // so we only test it for NULL and never dereference it.
            let module = &*std::ptr::addr_of!(crate::module::ngx_http_vts_module);
            let ctx = *(*r).ctx.add(module.ctx_index);
            if !ctx.is_null() {
                return Status::NGX_DECLINED;
            }

            // The elapsed request time is the same for the server
            // zone and for every upstream attempt, so read the clock
            // once.
            let request_time =
                crate::calculate_request_time((*r).start_sec as u64, (*r).start_msec as u64);

            record_server_zone(r, request_time);
            record_upstream_and_cache(r, request_time);
        }

        Status::NGX_DECLINED
    }

    fn name() -> &'static str {
        "vts log"
    }
}

/// Server-zone update, performed for every main request.
///
/// # Safety
///
/// `r` must be a live `ngx_http_request_t`.
unsafe fn record_server_zone(r: *const ngx_http_request_t, request_time: u64) {
    // Key on the matched server block's first `server_name` rather
    // than the raw `Host` header (`r->headers_in.server`): that
    // header is attacker-controlled and has unbounded cardinality,
    // which would let any client trivially blow up the shared table
    // by sending varying Host values.
    //
    // A name too long to be a key falls back to the default zone
    // instead of being dropped: a counter that reads high is easier to
    // notice than one that silently stops. See `t/011.long_names.t`.
    let server_zone = NgxHttpCoreModule::server_conf(&*r)
        .and_then(|cscf| as_key(&cscf.server_name))
        .unwrap_or(DEFAULT_SERVER_ZONE);

    let status = match (*r).headers_out.status {
        0 => 200,
        s => s as u16,
    };

    let bytes_in = to_u64((*r).request_length);
    let bytes_out = (*r).connection.as_ref().map_or(0, |c| to_u64(c.sent));

    crate::track_server_request(server_zone, status, bytes_in, bytes_out, request_time);
}

/// Upstream and cache updates, performed only when the request went
/// through the upstream framework.
///
/// # Safety
///
/// `r` must be a live `ngx_http_request_t`.
unsafe fn record_upstream_and_cache(r: *const ngx_http_request_t, request_time: u64) {
    let Some(u) = (*r).upstream.as_ref() else {
        return;
    };

    // Get upstream name from the upstream configuration.
    let upstream_name = u
        .conf
        .as_ref()
        .and_then(|conf| conf.upstream.as_ref())
        .and_then(|uscf| as_key(&uscf.host));

    // Walk `r->upstream_states` so each upstream attempt is recorded
    // as its own sample.  For requests with no retry this is one
    // iteration; for retries (e.g. a 502 from peer A followed by a
    // 200 from peer B) we record both attempts instead of only the
    // final one.
    //
    // (`u->state` is just a pointer to the in-progress entry in this
    // same array; the array itself hangs off the request struct.)
    //
    // Entries whose `peer` is NULL or empty are skipped: that's the
    // cache-HIT path where `r->upstream` exists but no peer was ever
    // contacted, plus init-time slots before peer selection.
    if let (Some(upstream_name), Some(states)) = (upstream_name, (*r).upstream_states.as_ref()) {
        let elts = states.elts as *const ngx_http_upstream_state_t;
        for i in 0..states.nelts {
            let st = &*elts.add(i);
            let Some(peer) = st.peer.as_ref().and_then(|p| as_key(p)) else {
                continue;
            };

            crate::track_upstream_request(
                upstream_name,
                peer,
                request_time,
                st.response_time as u64,
                to_u64(st.bytes_sent),
                to_u64(st.bytes_received),
                st.status as u16,
            );
        }
    }

    #[cfg(ngx_feature = "http_cache")]
    record_cache(u, r);
}

/// Record a `$upstream_cache_status` observation.
///
/// `cache_status == 0` means the request did not consult any cache (no
/// `proxy_cache` configured, or the request bypassed cache lookup
/// before nginx assigned a status).  Cache zone name is the shared
/// memory zone declared by `proxy_cache_path ... keys_zone=NAME:SIZE`.
///
/// We also forward `max_size` and the current `used_size` so
/// `nginx_vts_cache_size_bytes{type="max"}` / `{type="used"}` reflect
/// reality.  Note that both `fc->max_size` and `fc->sh->size` are kept
/// in **cache blocks** by nginx internally (the file cache manager
/// divides `max_size` by `bsize` during init for direct comparison
/// against `sh->size`), so we multiply each by `bsize` to recover
/// bytes.
///
/// # Safety
///
/// `u` and `r` must be the live upstream and request structs.
#[cfg(ngx_feature = "http_cache")]
unsafe fn record_cache(u: &ngx_http_upstream_t, r: *const ngx_http_request_t) {
    let cache_status = u.cache_status();
    if cache_status == 0 {
        return;
    }

    let Some(fc) = (*r).cache.as_ref().and_then(|c| c.file_cache.as_ref()) else {
        return;
    };
    let Some(zone) = fc.shm_zone.as_ref().and_then(|z| as_key(&z.shm.name)) else {
        return;
    };

    let bsize = fc.bsize as u64;
    let max_size = to_u64(fc.max_size) * bsize;
    let used_size = fc.sh.as_ref().map_or(0, |sh| to_u64(sh.size) * bsize);

    crate::track_cache_status(zone, cache_status as u8, max_size, used_size);
}

/// Register the LOG_PHASE handler.  Must be called from the module's
/// `postconfiguration`.
///
pub(crate) fn register(cf: &mut ngx_conf_t) -> bool {
    add_phase_handler::<VtsLogHandler>(cf).is_ok()
}
