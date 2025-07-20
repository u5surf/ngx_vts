use ngx::ffi::*;
use ngx::{core, http, log, Status};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use crate::stats::{VtsStats, VtsStatsManager};
use crate::config::VtsConfig;

pub struct VtsHandler;

impl VtsHandler {
    pub extern "C" fn vts_status_handler(r: *mut ngx_http_request_t) -> ngx_int_t {
        unsafe {
            // Get location configuration
            let loc_conf = ngx_http_get_module_loc_conf(r, &ngx_http_vts_module as *const _ as *mut _) as *mut VtsConfig;
            if loc_conf.is_null() || !(*loc_conf).enable_status {
                return NGX_HTTP_NOT_FOUND as ngx_int_t;
            }

            // Get stats manager from global state
            if let Some(ref manager) = crate::VTS_MANAGER {
                let stats = manager.get_stats();
                Self::handle_prometheus_response(r, &stats)
            } else {
                NGX_HTTP_INTERNAL_SERVER_ERROR as ngx_int_t
            }
        }
    }

    unsafe fn handle_prometheus_response(r: *mut ngx_http_request_t, stats: &VtsStats) -> ngx_int_t {
        let prometheus_content = Self::generate_prometheus_metrics(stats);
        
        let content_type = ngx_string!("text/plain; version=0.0.4; charset=utf-8");
        (*r).headers_out.content_type = content_type;
        (*r).headers_out.content_type_len = content_type.len;

        Self::send_response(r, prometheus_content.as_bytes())
    }


    unsafe fn send_response(r: *mut ngx_http_request_t, content: &[u8]) -> ngx_int_t {
        // Set status
        (*r).headers_out.status = NGX_HTTP_OK;
        (*r).headers_out.content_length_n = content.len() as ngx_off_t;

        // Send headers
        let rc = ngx_http_send_header(r);
        if rc == NGX_ERROR as ngx_int_t || rc > NGX_OK as ngx_int_t {
            return rc;
        }

        // Create buffer chain
        let pool = (*r).pool;
        let buf = ngx_create_temp_buf(pool, content.len());
        if buf.is_null() {
            return NGX_HTTP_INTERNAL_SERVER_ERROR as ngx_int_t;
        }

        // Copy content to buffer
        ptr::copy_nonoverlapping(content.as_ptr(), (*buf).pos, content.len());
        (*buf).last = (*buf).pos.add(content.len());
        (*buf).last_buf = 1;
        (*buf).last_in_chain = 1;

        // Create chain link
        let out = ngx_alloc_chain_link(pool);
        if out.is_null() {
            return NGX_HTTP_INTERNAL_SERVER_ERROR as ngx_int_t;
        }

        (*out).buf = buf;
        (*out).next = ptr::null_mut();

        // Send output
        ngx_http_output_filter(r, out)
    }

    fn generate_prometheus_metrics(stats: &VtsStats) -> String {
        let mut metrics = String::new();
        
        // Add HELP and TYPE comments for Prometheus
        metrics.push_str("# HELP nginx_vts_info Nginx VTS module information\n");
        metrics.push_str("# TYPE nginx_vts_info gauge\n");
        metrics.push_str(&format!("nginx_vts_info{{hostname=\"{}\",version=\"{}\"}} 1\n", stats.hostname, stats.version));
        
        // Connection metrics
        metrics.push_str("# HELP nginx_vts_connections Current nginx connections\n");
        metrics.push_str("# TYPE nginx_vts_connections gauge\n");
        metrics.push_str(&format!("nginx_vts_connections{{state=\"active\"}} {}\n", stats.connections.active));
        metrics.push_str(&format!("nginx_vts_connections{{state=\"reading\"}} {}\n", stats.connections.reading));
        metrics.push_str(&format!("nginx_vts_connections{{state=\"writing\"}} {}\n", stats.connections.writing));
        metrics.push_str(&format!("nginx_vts_connections{{state=\"waiting\"}} {}\n", stats.connections.waiting));
        
        metrics.push_str("# HELP nginx_vts_connections_total Total nginx connections\n");
        metrics.push_str("# TYPE nginx_vts_connections_total counter\n");
        metrics.push_str(&format!("nginx_vts_connections_total{{state=\"accepted\"}} {}\n", stats.connections.accepted));
        metrics.push_str(&format!("nginx_vts_connections_total{{state=\"handled\"}} {}\n", stats.connections.handled));

        // Server zone metrics
        if !stats.server_zones.is_empty() {
            metrics.push_str("# HELP nginx_vts_server_requests_total Total number of requests\n");
            metrics.push_str("# TYPE nginx_vts_server_requests_total counter\n");
            
            metrics.push_str("# HELP nginx_vts_server_bytes_total Total bytes transferred\n");
            metrics.push_str("# TYPE nginx_vts_server_bytes_total counter\n");
            
            metrics.push_str("# HELP nginx_vts_server_responses_total Total responses by status code\n");
            metrics.push_str("# TYPE nginx_vts_server_responses_total counter\n");
            
            metrics.push_str("# HELP nginx_vts_server_request_seconds Request processing time\n");
            metrics.push_str("# TYPE nginx_vts_server_request_seconds gauge\n");
            
            for (zone, server_stats) in &stats.server_zones {
                let zone_label = format!("{{zone=\"{}\"}}", zone);
                
                // Request count
                metrics.push_str(&format!("nginx_vts_server_requests_total{} {}\n", zone_label, server_stats.requests));
                
                // Bytes transferred
                metrics.push_str(&format!("nginx_vts_server_bytes_total{{zone=\"{}\",direction=\"in\"}} {}\n", zone, server_stats.bytes_in));
                metrics.push_str(&format!("nginx_vts_server_bytes_total{{zone=\"{}\",direction=\"out\"}} {}\n", zone, server_stats.bytes_out));
                
                // Response status metrics
                metrics.push_str(&format!("nginx_vts_server_responses_total{{zone=\"{}\",status=\"1xx\"}} {}\n", zone, server_stats.responses.status_1xx));
                metrics.push_str(&format!("nginx_vts_server_responses_total{{zone=\"{}\",status=\"2xx\"}} {}\n", zone, server_stats.responses.status_2xx));
                metrics.push_str(&format!("nginx_vts_server_responses_total{{zone=\"{}\",status=\"3xx\"}} {}\n", zone, server_stats.responses.status_3xx));
                metrics.push_str(&format!("nginx_vts_server_responses_total{{zone=\"{}\",status=\"4xx\"}} {}\n", zone, server_stats.responses.status_4xx));
                metrics.push_str(&format!("nginx_vts_server_responses_total{{zone=\"{}\",status=\"5xx\"}} {}\n", zone, server_stats.responses.status_5xx));
                
                // Request time metrics
                metrics.push_str(&format!("nginx_vts_server_request_seconds{{zone=\"{}\",type=\"total\"}} {}\n", zone, server_stats.request_times.total));
                metrics.push_str(&format!("nginx_vts_server_request_seconds{{zone=\"{}\",type=\"avg\"}} {}\n", zone, server_stats.request_times.avg));
                metrics.push_str(&format!("nginx_vts_server_request_seconds{{zone=\"{}\",type=\"min\"}} {}\n", zone, server_stats.request_times.min));
                metrics.push_str(&format!("nginx_vts_server_request_seconds{{zone=\"{}\",type=\"max\"}} {}\n", zone, server_stats.request_times.max));
            }
        }

        metrics
    }
}