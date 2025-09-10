/*
 * nginx VTS module main definition
 * 
 * This file defines the nginx module structure and integrates with
 * the Rust implementation via the C wrapper.
 */

#include <ngx_config.h>
#include <ngx_core.h>
#include <ngx_http.h>

// Forward declarations from wrapper
extern ngx_int_t ngx_http_vts_init_wrapper(ngx_conf_t *cf);

// Configuration structure
typedef struct {
    ngx_flag_t enable;
    size_t zone_size;
    ngx_str_t zone_name;
} ngx_http_vts_loc_conf_t;

// Forward declarations
static ngx_int_t ngx_http_vts_postconfiguration(ngx_conf_t *cf);
static void *ngx_http_vts_create_loc_conf(ngx_conf_t *cf);
static char *ngx_http_vts_merge_loc_conf(ngx_conf_t *cf, void *parent, void *child);
static char *ngx_http_vts_zone_directive(ngx_conf_t *cf, ngx_command_t *cmd, void *conf);
static char *ngx_http_vts_status_directive(ngx_conf_t *cf, ngx_command_t *cmd, void *conf);
static char *ngx_http_vts_upstream_stats_directive(ngx_conf_t *cf, ngx_command_t *cmd, void *conf);

// Handler declaration
static ngx_int_t ngx_http_vts_status_handler(ngx_http_request_t *r);

// Module commands
static ngx_command_t ngx_http_vts_commands[] = {
    {
        ngx_string("vts_zone"),
        NGX_HTTP_MAIN_CONF | NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF | NGX_CONF_TAKE2,
        ngx_http_vts_zone_directive,
        NGX_HTTP_LOC_CONF_OFFSET,
        0,
        NULL
    },
    {
        ngx_string("vts_status"),
        NGX_HTTP_LOC_CONF | NGX_CONF_NOARGS,
        ngx_http_vts_status_directive,
        NGX_HTTP_LOC_CONF_OFFSET,
        0,
        NULL
    },
    {
        ngx_string("vts_upstream_stats"),
        NGX_HTTP_MAIN_CONF | NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF | NGX_CONF_FLAG,
        ngx_http_vts_upstream_stats_directive,
        NGX_HTTP_LOC_CONF_OFFSET,
        offsetof(ngx_http_vts_loc_conf_t, enable),
        NULL
    },
    ngx_null_command
};

// Module context
static ngx_http_module_t ngx_http_vts_module_ctx = {
    NULL,                              /* preconfiguration */
    ngx_http_vts_postconfiguration,    /* postconfiguration */
    NULL,                              /* create main configuration */
    NULL,                              /* init main configuration */
    NULL,                              /* create server configuration */
    NULL,                              /* merge server configuration */
    ngx_http_vts_create_loc_conf,      /* create location configuration */
    ngx_http_vts_merge_loc_conf        /* merge location configuration */
};

// Module definition
ngx_module_t ngx_http_vts_module = {
    NGX_MODULE_V1,
    &ngx_http_vts_module_ctx,          /* module context */
    ngx_http_vts_commands,             /* module directives */
    NGX_HTTP_MODULE,                   /* module type */
    NULL,                              /* init master */
    NULL,                              /* init module */
    NULL,                              /* init process */
    NULL,                              /* init thread */
    NULL,                              /* exit thread */
    NULL,                              /* exit process */
    NULL,                              /* exit master */
    NGX_MODULE_V1_PADDING
};

// Status handler implementation
static ngx_int_t
ngx_http_vts_status_handler(ngx_http_request_t *r)
{
    ngx_int_t rc;
    ngx_buf_t *b;
    ngx_chain_t out;
    
    // Rust function to get status output
    extern const char* ngx_http_vts_get_status();
    
    if (!(r->method & (NGX_HTTP_GET|NGX_HTTP_HEAD))) {
        return NGX_HTTP_NOT_ALLOWED;
    }
    
    rc = ngx_http_discard_request_body(r);
    if (rc != NGX_OK) {
        return rc;
    }
    
    // Get status from Rust implementation
    const char *status_output = ngx_http_vts_get_status();
    size_t status_len = ngx_strlen(status_output);
    
    // Set response headers
    r->headers_out.status = NGX_HTTP_OK;
    r->headers_out.content_length_n = status_len;
    
    if (r->method == NGX_HTTP_HEAD) {
        rc = ngx_http_send_header(r);
        if (rc == NGX_ERROR || rc > NGX_OK || r->header_only) {
            return rc;
        }
    }
    
    // Create response buffer
    b = ngx_create_temp_buf(r->pool, status_len);
    if (b == NULL) {
        return NGX_HTTP_INTERNAL_SERVER_ERROR;
    }
    
    ngx_memcpy(b->pos, status_output, status_len);
    b->last = b->pos + status_len;
    b->last_buf = 1;
    b->last_in_chain = 1;
    
    // Set output chain
    out.buf = b;
    out.next = NULL;
    
    // Send headers
    rc = ngx_http_send_header(r);
    if (rc == NGX_ERROR || rc > NGX_OK || r->header_only) {
        return rc;
    }
    
    // Send body
    return ngx_http_output_filter(r, &out);
}

// Postconfiguration - called after all configuration is parsed
static ngx_int_t
ngx_http_vts_postconfiguration(ngx_conf_t *cf)
{
    // Initialize the wrapper (includes LOG_PHASE handler registration)
    return ngx_http_vts_init_wrapper(cf);
}

// Create location configuration
static void *
ngx_http_vts_create_loc_conf(ngx_conf_t *cf)
{
    ngx_http_vts_loc_conf_t *conf;
    
    conf = ngx_pcalloc(cf->pool, sizeof(ngx_http_vts_loc_conf_t));
    if (conf == NULL) {
        return NULL;
    }
    
    conf->enable = NGX_CONF_UNSET;
    conf->zone_size = NGX_CONF_UNSET_SIZE;
    
    return conf;
}

// Merge location configurations
static char *
ngx_http_vts_merge_loc_conf(ngx_conf_t *cf, void *parent, void *child)
{
    ngx_http_vts_loc_conf_t *prev = parent;
    ngx_http_vts_loc_conf_t *conf = child;
    
    ngx_conf_merge_value(conf->enable, prev->enable, 0);
    ngx_conf_merge_size_value(conf->zone_size, prev->zone_size, 1024*1024);
    
    return NGX_CONF_OK;
}

// Handle vts_zone directive
static char *
ngx_http_vts_zone_directive(ngx_conf_t *cf, ngx_command_t *cmd, void *conf)
{
    (void)cmd; // Mark as intentionally unused
    (void)conf; // Mark as intentionally unused
    (void)cf;  // For now, we don't use the configuration
    
    // For now, just accept the directive
    // The Rust side handles the actual zone setup
    return NGX_CONF_OK;
}

// Handle vts_status directive
static char *
ngx_http_vts_status_directive(ngx_conf_t *cf, ngx_command_t *cmd, void *conf)
{
    ngx_http_core_loc_conf_t *clcf;
    
    (void)cmd;  // Mark as intentionally unused
    (void)conf; // Mark as intentionally unused
    
    clcf = ngx_http_conf_get_module_loc_conf(cf, ngx_http_core_module);
    clcf->handler = ngx_http_vts_status_handler;
    
    return NGX_CONF_OK;
}

// Handle vts_upstream_stats directive
static char *
ngx_http_vts_upstream_stats_directive(ngx_conf_t *cf, ngx_command_t *cmd, void *conf)
{
    (void)cf;   // Mark as intentionally unused
    (void)cmd;  // Mark as intentionally unused
    (void)conf; // Mark as intentionally unused
    
    // For now, just accept the directive
    // The LOG_PHASE handler will handle upstream tracking
    return NGX_CONF_OK;
}