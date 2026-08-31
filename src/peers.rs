//! Reading the peers an upstream group holds.
//!
//! The counters elsewhere come from `r->upstream_states`, which only says what
//! a request did. It cannot say whether a peer is currently taken out of
//! rotation, and it never mentions a peer that has served nothing. Both of
//! those come from the group itself, so this walks `uscf->peer.data`.
//!
//! This is what `nginx_vts_upstream_server_up` needs. Before it existed the
//! gauge read 1 for every peer including ones nothing was listening on, since
//! nothing ever set the flag it was derived from.

#[cfg(not(test))]
use std::collections::HashMap;

#[cfg(not(test))]
use ngx::ffi::{
    ngx_http_upstream_module, ngx_http_upstream_rr_peers_t, ngx_http_upstream_srv_conf_t,
    ngx_rwlock_rlock, ngx_rwlock_unlock,
};
#[cfg(not(test))]
use ngx::http::Request;

/// What the group says about one of its peers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PeerState {
    /// Out of rotation: either configured `down`, or failed often enough that
    /// nginx has taken it out for now.
    pub down: bool,
    /// Declared with the `backup` parameter.
    pub backup: bool,
    /// The `weight` parameter.
    pub weight: i64,
    /// The `max_fails` parameter. Zero turns failure counting off.
    pub max_fails: u64,
    /// Failures counted within the current `fail_timeout`.
    pub fails: u64,
}

/// Every peer of every upstream group, keyed by `(group, peer address)`.
///
/// Returns an empty map when the upstream module is not configured, which is
/// the case for a server that proxies nothing.
#[cfg(not(test))]
pub fn collect(request: &Request) -> HashMap<(String, String), PeerState> {
    let mut out = HashMap::new();

    // SAFETY: the request holds a valid configuration for the cycle it belongs
    // to; `ngx_http_get_module_main_conf` is a field lookup on it.
    let umcf = unsafe {
        let ctx = request.as_ref().main_conf;
        if ctx.is_null() {
            return out;
        }
        let idx = ngx_http_upstream_module.ctx_index;
        let umcf = *ctx.add(idx) as *mut ngx::ffi::ngx_http_upstream_main_conf_t;
        match umcf.as_ref() {
            Some(u) => u,
            None => return out,
        }
    };

    // SAFETY: `upstreams` is an array of `ngx_http_upstream_srv_conf_t *`
    // built during configuration and not written again.
    let uscfp: &[*mut ngx_http_upstream_srv_conf_t] = unsafe { umcf.upstreams.as_slice() };

    for &uscf in uscfp {
        // SAFETY: the array holds valid pointers for the life of the cycle.
        let Some(uscf) = (unsafe { uscf.as_ref() }) else {
            continue;
        };

        // SAFETY: `host` is the group's name, allocated from the
        // configuration pool.
        let group = unsafe { ngx::core::NgxStr::from_ngx_str(uscf.host) }
            .to_string_lossy()
            .into_owned();

        let peers = uscf.peer.data as *mut ngx_http_upstream_rr_peers_t;
        // SAFETY: set by `ngx_http_upstream_init_round_robin`. A group using a
        // different balancer leaves something else here, so anything that is
        // not round robin is skipped rather than reinterpreted.
        let Some(peers) = (unsafe { peers.as_ref() }) else {
            continue;
        };

        collect_group(&group, peers, &mut out);
    }

    out
}

/// Walks one group's primary list and then its backup list.
#[cfg(not(test))]
fn collect_group(
    group: &str,
    peers: &ngx_http_upstream_rr_peers_t,
    out: &mut HashMap<(String, String), PeerState>,
) {
    let mut list = Some(peers);
    let mut backup = false;

    while let Some(peers) = list {
        // The lock only exists for a group with a `zone`, where the peers live
        // in shared memory and another worker may be writing `fails`. This is
        // what the ngx_http_upstream_rr_peers_rlock macro expands to.
        let locked = !peers.shpool.is_null();
        if locked {
            // SAFETY: the rwlock belongs to this peers list and lives with it.
            unsafe { ngx_rwlock_rlock(&peers.rwlock as *const _ as *mut _) };
        }

        let mut peer = peers.peer;
        // SAFETY: the list is walked under the lock above where one is needed,
        // and its links are stable otherwise.
        while let Some(p) = unsafe { peer.as_ref() } {
            let name = unsafe { ngx::core::NgxStr::from_ngx_str(p.name) }
                .to_string_lossy()
                .into_owned();

            // What the original module derives, and for the same reason: a
            // peer is out either because the configuration says so, or because
            // nginx has counted enough failures to take it out. max_fails 0
            // turns the counting off, so the comparison has to be guarded --
            // without it `0 >= 0` holds from the first request and every such
            // peer reads as down.
            let down = p.down != 0 || (p.max_fails != 0 && p.fails >= p.max_fails);

            out.insert(
                (group.to_owned(), name),
                PeerState {
                    down,
                    backup,
                    weight: p.weight as i64,
                    max_fails: p.max_fails as u64,
                    fails: p.fails as u64,
                },
            );

            peer = p.next;
        }

        if locked {
            // SAFETY: paired with the lock taken above.
            unsafe { ngx_rwlock_unlock(&peers.rwlock as *const _ as *mut _) };
        }

        // SAFETY: `next` is the backup list, or null.
        list = unsafe { peers.next.as_ref() };
        backup = true;
    }
}
