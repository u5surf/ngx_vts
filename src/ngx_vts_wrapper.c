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

// External Rust initialization function  
extern ngx_int_t ngx_http_vts_init_rust_module(ngx_conf_t *cf);

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
    ngx_http_upstream_state_t *state;
    ngx_str_t upstream_name = ngx_null_string;
    ngx_str_t server_addr = ngx_null_string; 
    ngx_msec_t upstream_response_time = 0;
    off_t bytes_sent = 0;
    off_t bytes_received = 0;
    ngx_uint_t status_code = 0;
    u_char upstream_name_buf[256];
    u_char server_addr_buf[256];

    // Debug log: LOG_PHASE handler called
    ngx_log_error(NGX_LOG_NOTICE, r->connection->log, 0,
                  "VTS LOG_PHASE handler called for request: %V", &r->uri);

    // Only process requests that used upstream
    u = r->upstream;
    if (u == NULL) {
        ngx_log_error(NGX_LOG_NOTICE, r->connection->log, 0,
                      "VTS LOG_PHASE: No upstream found for request");
        return NGX_DECLINED;
    }

    ngx_log_error(NGX_LOG_NOTICE, r->connection->log, 0,
                  "VTS LOG_PHASE: Found upstream for request");

    // Get upstream name from the upstream configuration
    if (u->conf && u->conf->upstream) {
        upstream_name = u->conf->upstream->host;
    }

    // Extract upstream state information
    state = u->state;
    if (state != NULL) {
        // Get server address
        if (state->peer) {
            server_addr = *state->peer;
        }

        // Get timing information
        upstream_response_time = state->response_time;
        // Get byte counts
        bytes_sent = state->bytes_sent;
        bytes_received = state->bytes_received;

        // Get status code
        status_code = state->status;
    }

    // Fallback to request-level information if upstream state is incomplete
    if (status_code == 0) {
        status_code = r->headers_out.status;
    }

    // Request time calculation is now handled in Rust side using ngx_timeofday()

    // Convert nginx strings to C strings for Rust FFI
    if (upstream_name.len > 0 && upstream_name.len < sizeof(upstream_name_buf) - 1) {
        ngx_memcpy(upstream_name_buf, upstream_name.data, upstream_name.len);
        upstream_name_buf[upstream_name.len] = '\0';
    } else {
        ngx_cpystrn(upstream_name_buf, (u_char*)"default", sizeof(upstream_name_buf));
    }

    if (server_addr.len > 0 && server_addr.len < sizeof(server_addr_buf) - 1) {
        ngx_memcpy(server_addr_buf, server_addr.data, server_addr.len);
        server_addr_buf[server_addr.len] = '\0';
    } else {
        ngx_cpystrn(server_addr_buf, (u_char*)"unknown", sizeof(server_addr_buf));
    }

    // Update server zone statistics for all requests
    u_char server_name_buf[256];
    ngx_str_t *server_name = &r->headers_in.server;
    if (server_name->len > 0 && server_name->len < sizeof(server_name_buf) - 1) {
        ngx_memcpy(server_name_buf, server_name->data, server_name->len);
        server_name_buf[server_name->len] = '\0';
    } else {
        ngx_cpystrn(server_name_buf, (u_char*)"_", sizeof(server_name_buf));
    }

    // Calculate total request time in milliseconds using nginx's builtin calculation
    ngx_msec_t request_time = 0;
    if (r->connection->log->action) {
        // Use nginx's internal request timing if available
        ngx_time_t *tp = ngx_timeofday();
        request_time = (ngx_msec_t) ((tp->sec - r->start_sec) * 1000 + (tp->msec - r->start_msec));
    }
    
    // Get response status (use r->headers_out.status if available, otherwise default)
    ngx_uint_t response_status = r->headers_out.status ? r->headers_out.status : status_code;
    if (response_status == 0) {
        response_status = 200; // Default to 200 if no status available
    }

    // Calculate bytes sent and received for this request
    off_t bytes_in = r->request_length;
    off_t bytes_out = r->connection->sent;
    
    // Call Rust function to update server zone statistics
    ngx_log_error(NGX_LOG_NOTICE, r->connection->log, 0,
                  "VTS LOG_PHASE: Updating server stats - server: %s, status: %d, bytes_in: %O, bytes_out: %O",
                  server_name_buf, response_status, bytes_in, bytes_out);

    vts_update_server_stats_ffi(
        (const char*)server_name_buf,
        (uint16_t)response_status,
        (uint64_t)bytes_in,
        (uint64_t)bytes_out,
        (uint64_t)request_time
    );

    // Call Rust function to update upstream statistics (if upstream exists)
    if (upstream_name.len > 0) {
        ngx_log_error(NGX_LOG_NOTICE, r->connection->log, 0,
                      "VTS LOG_PHASE: Calling vts_track_upstream_request - upstream: %s, server: %s, status: %d",
                      upstream_name_buf, server_addr_buf, status_code);
                      
        vts_track_upstream_request(
            (const char*)upstream_name_buf,
            (const char*)server_addr_buf,
            (uint64_t)r->start_sec,
            (uint64_t)r->start_msec,
            (uint64_t)upstream_response_time,
            (uint64_t)bytes_sent,
            (uint64_t)bytes_received,
            (uint16_t)status_code
        );
    }
    
    ngx_log_error(NGX_LOG_NOTICE, r->connection->log, 0,
                  "VTS LOG_PHASE: vts_track_upstream_request completed");

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
