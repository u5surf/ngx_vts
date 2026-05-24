//! Pure-Rust accessor for nginx's `ngx_stat_*` global atomics.
//!
//! These seven atomics back the `stub_status` module's response and are
//! populated by nginx core when (and only when) `NGX_STAT_STUB` is
//! defined — i.e. when nginx is built with
//! `--with-http_stub_status_module`.  When they're available we read
//! them directly to surface accurate `nginx_vts_connections{state=…}`
//! counters; when they aren't, the caller falls back to a
//! coarser cycle-table walk.
//!
//! The symbols are looked up at runtime via `dlsym(RTLD_DEFAULT, …)`
//! rather than declared as `extern "C" static` so that the cdylib
//! still loads on nginx builds that omit `stub_status` — declaring
//! them as plain externs would make the dynamic linker fail at module
//! load time, taking nginx down with it.
//!
//! The lookup is cached in a `OnceLock` so the cost is paid exactly
//! once per worker.

#[cfg(not(test))]
use ngx::ffi::ngx_atomic_t;
#[cfg(not(test))]
use std::ffi::CStr;
#[cfg(not(test))]
use std::sync::OnceLock;

/// One snapshot of nginx's connection-state counters.  All values are
/// taken from the same source as `stub_status`, but because each
/// atomic is read independently the snapshot is not consistent across
/// counters.  That's fine for monitoring — the drift between reads is
/// sub-microsecond.
#[derive(Clone, Copy, Debug, Default)]
#[allow(dead_code)] // `requests` is exposed for symmetry with stub_status; not yet plumbed into a metric.
pub struct ConnectionStats {
    pub active: u64,
    pub reading: u64,
    pub writing: u64,
    pub waiting: u64,
    pub accepted: u64,
    pub handled: u64,
    pub requests: u64,
}

/// Resolved pointers to the seven `ngx_stat_*` atomics.  Populated
/// once via `dlsym`; remains `None` on nginx builds without
/// `stub_status`.
#[cfg(not(test))]
struct StatPointers {
    active: *const ngx_atomic_t,
    reading: *const ngx_atomic_t,
    writing: *const ngx_atomic_t,
    waiting: *const ngx_atomic_t,
    accepted: *const ngx_atomic_t,
    handled: *const ngx_atomic_t,
    requests: *const ngx_atomic_t,
}

// SAFETY: the pointers are read-only and stable for the lifetime of
// the process — nginx allocates them once before workers fork.
#[cfg(not(test))]
unsafe impl Send for StatPointers {}
#[cfg(not(test))]
unsafe impl Sync for StatPointers {}

#[cfg(not(test))]
static STAT_POINTERS: OnceLock<Option<StatPointers>> = OnceLock::new();

#[cfg(not(test))]
fn lookup_symbol(name: &CStr) -> Option<*const ngx_atomic_t> {
    // The nginx symbols are global variables of type `ngx_atomic_t *`
    // (a pointer to the atomic in shared memory), so `dlsym` returns
    // the address of that pointer variable — not the atomic itself.
    // Dereference once here so the cached value is the atomic's
    // address, ready for direct reads on the hot path.
    //
    // SAFETY: `dlsym` is always safe to call with a valid CStr.  When
    // it returns non-null, the underlying global is the
    // `ngx_atomic_t *` allocated and assigned once by nginx core
    // before workers fork — a single read here is sound.
    let sym_addr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, name.as_ptr()) };
    if sym_addr.is_null() {
        return None;
    }
    let atomic_ptr = unsafe { *(sym_addr as *const *const ngx_atomic_t) };
    if atomic_ptr.is_null() {
        return None;
    }
    Some(atomic_ptr)
}

#[cfg(not(test))]
fn resolve_pointers() -> Option<StatPointers> {
    // All seven must resolve; if any is missing we treat the build as
    // not having stub_status compiled in (rather than silently zeroing
    // out a subset of the counters).
    Some(StatPointers {
        active: lookup_symbol(c"ngx_stat_active")?,
        reading: lookup_symbol(c"ngx_stat_reading")?,
        writing: lookup_symbol(c"ngx_stat_writing")?,
        waiting: lookup_symbol(c"ngx_stat_waiting")?,
        accepted: lookup_symbol(c"ngx_stat_accepted")?,
        handled: lookup_symbol(c"ngx_stat_handled")?,
        requests: lookup_symbol(c"ngx_stat_requests")?,
    })
}

/// Sample all seven counters in one call.  Returns `None` when nginx
/// was built without `stub_status` and the symbols cannot be found.
#[cfg(not(test))]
#[allow(clippy::unnecessary_cast)] // `ngx_atomic_t` is `c_ulong`, which is 32-bit on some targets.
pub fn read() -> Option<ConnectionStats> {
    let ptrs = STAT_POINTERS.get_or_init(resolve_pointers).as_ref()?;
    // SAFETY: `ptrs` are non-null pointers into nginx's global state,
    // populated once before workers fork and never freed.  The atomics
    // themselves are `volatile ngx_atomic_uint_t` on the C side, so a
    // plain read here observes the latest committed value (this is the
    // same access pattern `stub_status` itself uses).
    unsafe {
        Some(ConnectionStats {
            active: *ptrs.active as u64,
            reading: *ptrs.reading as u64,
            writing: *ptrs.writing as u64,
            waiting: *ptrs.waiting as u64,
            accepted: *ptrs.accepted as u64,
            handled: *ptrs.handled as u64,
            requests: *ptrs.requests as u64,
        })
    }
}

/// Test-only stub.  The unit-test binary doesn't link against an nginx
/// that exports the atomics, so we pretend the build doesn't have
/// `stub_status` and let callers exercise their fallback path.
#[cfg(test)]
pub fn read() -> Option<ConnectionStats> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_returns_none_in_tests() {
        // The test binary never links the nginx atomics.
        assert!(read().is_none());
    }
}
