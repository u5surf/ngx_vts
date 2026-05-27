/*
 * nginx VTS module C wrapper for Rust implementation
 * 
 * This file provides the necessary C integration to register LOG_PHASE handlers
 * and bridge nginx requests to the Rust VTS implementation.
 */

#include <ngx_config.h>
#include <ngx_core.h>
#include <ngx_http.h>

// External Rust functions
extern void vts_track_upstream_request(
    const char* upstream_name,
    const char* server_addr,
    uint64_t start_sec,
    uint64_t start_msec,
    uint64_t upstream_response_time,
    uint64_t bytes_sent,
    uint64_t bytes_received,
    uint16_t status_code
);

// External Rust functions
extern void vts_update_server_stats_ffi(
    const char* server_name,
    uint16_t status,
    uint64_t bytes_in,
    uint64_t bytes_out,
    uint64_t request_time
);

extern void vts_update_cache_stats_ffi(
    const char* zone_name,
    uint8_t cache_status,
    uint64_t max_size,
    uint64_t used_size
);

// External Rust initialization function
extern ngx_int_t ngx_http_vts_init_rust_module(ngx_conf_t *cf);

// Module struct defined in ngx_http_vts_module.c.  We consult its
// per-request ctx slot to detect requests served by the vts_status
// content handler (so Prometheus scrapes don't inflate server_zone
// counters).
extern ngx_module_t ngx_http_vts_module;

/*
 * LOG_PHASE handler implementation
 * 
 * This handler is called by nginx during the LOG_PHASE for each request.
 * It extracts upstream information and forwards it to the Rust implementation.
 */
static ngx_int_t
ngx_http_vts_log_handler(ngx_http_request_t *r)
{
    ngx_http_upstream_t *u;
    ngx_str_t upstream_name = ngx_null_string;
    u_char upstream_name_buf[256];
    u_char server_addr_buf[256];
    u_char server_name_buf[256];
    ngx_http_core_srv_conf_t *cscf;
    ngx_str_t server_zone;

    // Count each user-facing request exactly once.  nginx fires the
    // LOG_PHASE handler for every subrequest as well as the main
    // request (auth_request, addition, SSI, X-Accel-Redirect, …);
    // letting those through would double-count both server-zone and
    // upstream counters.  `r->main` always points at the top-level
    // request, so `r == r->main` selects exactly the main one.
    if (r != r->main) {
        return NGX_DECLINED;
    }

    // Skip Prometheus scrapes: the vts_status content handler sets
    // a non-NULL ctx on the request before rendering, which lets us
    // exclude /status from server_zone counters here.  Otherwise
    // every scrape would inflate `nginx_vts_server_requests_total`
    // for whichever vhost hosts /status.
    if (ngx_http_get_module_ctx(r, ngx_http_vts_module) != NULL) {
        return NGX_DECLINED;
    }

    // ----- server zone update (always for main requests) -----

    // Key on the matched server block's first `server_name` rather
    // than the raw `Host` header (`r->headers_in.server`): that
    // header is attacker-controlled and has unbounded cardinality,
    // which would let any client trivially blow up the shared table
    // by sending varying Host values.
    cscf = ngx_http_get_module_srv_conf(r, ngx_http_core_module);
    if (cscf != NULL && cscf->server_name.len > 0) {
        server_zone = cscf->server_name;
    } else {
        ngx_str_set(&server_zone, "_");
    }

    if (server_zone.len > 0 && server_zone.len < sizeof(server_name_buf) - 1) {
        ngx_memcpy(server_name_buf, server_zone.data, server_zone.len);
        server_name_buf[server_zone.len] = '\0';
    } else {
        ngx_cpystrn(server_name_buf, (u_char*)"_", sizeof(server_name_buf));
    }

    // Calculate total request time in milliseconds using nginx's builtin calculation
    ngx_msec_t request_time;
    ngx_time_t *tp = ngx_timeofday();
    request_time = (ngx_msec_t) ((tp->sec - r->start_sec) * 1000 + (tp->msec - r->start_msec));

    // Get response status (use r->headers_out.status if available, otherwise default)
    ngx_uint_t response_status = r->headers_out.status ? r->headers_out.status : 200;

    // Calculate bytes sent and received for this request
    off_t bytes_in = r->request_length;
    off_t bytes_out = r->connection->sent;

    vts_update_server_stats_ffi(
        (const char*)server_name_buf,
        (uint16_t)response_status,
        (uint64_t)bytes_in,
        (uint64_t)bytes_out,
        (uint64_t)request_time
    );

    // ----- upstream + cache updates (only when upstream framework was used) -----

    u = r->upstream;
    if (u == NULL) {
        return NGX_DECLINED;
    }

    // Get upstream name from the upstream configuration
    if (u->conf && u->conf->upstream) {
        upstream_name = u->conf->upstream->host;
    }

    // Convert upstream name to C string up front so the loop below can
    // reuse it across each `r->upstream_states` entry.
    if (upstream_name.len > 0 && upstream_name.len < sizeof(upstream_name_buf) - 1) {
        ngx_memcpy(upstream_name_buf, upstream_name.data, upstream_name.len);
        upstream_name_buf[upstream_name.len] = '\0';
    } else {
        upstream_name_buf[0] = '\0';
    }

    // Walk `r->upstream_states` so each upstream attempt is recorded
    // as its own sample.  For requests with no retry this is one
    // iteration and matches the previous single-state behavior; for
    // retries (e.g. a 502 from peer A followed by a 200 from peer B)
    // we now record both attempts instead of only the final one.
    //
    // (`u->state` is just a pointer to the in-progress entry in this
    // same array; the array itself hangs off the request struct.)
    //
    // Entries whose `peer` is NULL or empty are skipped: that's the
    // cache-HIT path where `r->upstream` exists but no peer was ever
    // contacted, plus init-time slots before peer selection.
    if (upstream_name.len > 0
        && r->upstream_states != NULL
        && r->upstream_states->nelts > 0)
    {
        ngx_http_upstream_state_t *states = r->upstream_states->elts;
        ngx_uint_t i;

        for (i = 0; i < r->upstream_states->nelts; i++) {
            ngx_http_upstream_state_t *st = &states[i];
            if (st->peer == NULL
                || st->peer->len == 0
                || st->peer->len >= sizeof(server_addr_buf))
            {
                continue;
            }
            ngx_memcpy(server_addr_buf, st->peer->data, st->peer->len);
            server_addr_buf[st->peer->len] = '\0';

            vts_track_upstream_request(
                (const char *)upstream_name_buf,
                (const char *)server_addr_buf,
                (uint64_t)r->start_sec,
                (uint64_t)r->start_msec,
                (uint64_t)st->response_time,
                (uint64_t)st->bytes_sent,
                (uint64_t)st->bytes_received,
                (uint16_t)st->status
            );
        }
    }

#if (NGX_HTTP_CACHE)
    // Record `$upstream_cache_status` observations.  `cache_status == 0`
    // means the request did not consult any cache (no `proxy_cache`
    // configured, or the request bypassed cache lookup before nginx
    // assigned a status), so skip it.  Cache zone name is the shared
    // memory zone declared by `proxy_cache_path ... keys_zone=NAME:SIZE`.
    //
    // We also forward `max_size` and the current `used_size` so
    // `nginx_vts_cache_size_bytes{type="max"}` / `{type="used"}`
    // reflect reality.  Note that both `fc->max_size` and
    // `fc->sh->size` are kept in **cache blocks** by nginx
    // internally (the file cache manager divides `max_size` by
    // `bsize` during init for direct comparison against `sh->size`),
    // so we multiply each by `bsize` to recover bytes.
    if (u->cache_status != 0
        && r->cache != NULL
        && r->cache->file_cache != NULL
        && r->cache->file_cache->shm_zone != NULL)
    {
        ngx_http_file_cache_t *fc = r->cache->file_cache;
        ngx_str_t *cz_name = &fc->shm_zone->shm.name;
        u_char cache_zone_buf[256];

        if (cz_name->len > 0 && cz_name->len < sizeof(cache_zone_buf) - 1) {
            ngx_memcpy(cache_zone_buf, cz_name->data, cz_name->len);
            cache_zone_buf[cz_name->len] = '\0';

            uint64_t bsize = (uint64_t) fc->bsize;
            uint64_t max_size = (uint64_t) fc->max_size * bsize;
            uint64_t used_size = 0;
            if (fc->sh != NULL) {
                used_size = (uint64_t) fc->sh->size * bsize;
            }

            vts_update_cache_stats_ffi(
                (const char *)cache_zone_buf,
                (uint8_t)u->cache_status,
                max_size,
                used_size
            );
        }
    }
#endif

    return NGX_DECLINED;
}

/*
 * Register LOG_PHASE handler
 * 
 * This function registers the LOG_PHASE handler with nginx.
 * It should be called during module initialization.
 */
ngx_int_t
ngx_http_vts_register_log_handler(ngx_conf_t *cf)
{
    ngx_http_handler_pt *h;
    ngx_http_core_main_conf_t *cmcf;

    // Get HTTP main configuration
    cmcf = ngx_http_conf_get_module_main_conf(cf, ngx_http_core_module);
    if (cmcf == NULL) {
        return NGX_ERROR;
    }

    // Add handler to LOG_PHASE
    h = ngx_array_push(&cmcf->phases[NGX_HTTP_LOG_PHASE].handlers);
    if (h == NULL) {
        return NGX_ERROR;
    }

    *h = ngx_http_vts_log_handler;

    return NGX_OK;
}

/*
 * Module initialization wrapper
 *
 * This function handles both C-side initialization (LOG_PHASE handler registration)
 * and Rust-side initialization.
 */
ngx_int_t
ngx_http_vts_init_wrapper(ngx_conf_t *cf)
{
    ngx_int_t rc;

    // Register LOG_PHASE handler (C implementation)
    rc = ngx_http_vts_register_log_handler(cf);
    if (rc != NGX_OK) {
        return rc;
    }

    // Initialize Rust module
    rc = ngx_http_vts_init_rust_module(cf);
    if (rc != NGX_OK) {
        return rc;
    }

    return NGX_OK;
}
