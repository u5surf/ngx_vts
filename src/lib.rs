use ngx::ffi::*;
use ngx::{core, http, log, Status};
use std::os::raw::{c_char, c_int, c_void};
use std::time::Instant;

mod stats;
mod handlers;
mod config;

use stats::{VtsStats, VtsStatsManager};
use handlers::VtsHandler;
use config::VtsConfig;

// Module definition
ngx::http_module! {
    name: ngx_http_vts_module,
    commands: [
        {
            name: b"vts_status\0",
            set: vts_set_status,
            conf: NGX_HTTP_LOC_CONF,
            args: NGX_CONF_NOARGS,
        },
        {
            name: b"vts_status_zone\0", 
            set: vts_set_status_zone,
            conf: NGX_HTTP_MAIN_CONF | NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF,
            args: NGX_CONF_FLAG,
        },
    ],
    ctx: VtsConfig::new(),
    init: Some(vts_init_module),
    init_process: Some(vts_init_process),
    postconfiguration: Some(vts_postconfiguration),
}

// Global statistics manager
static mut VTS_MANAGER: Option<VtsStatsManager> = None;

// Module initialization
extern "C" fn vts_init_module(cycle: *mut ngx_cycle_t) -> ngx_int_t {
    unsafe {
        VTS_MANAGER = Some(VtsStatsManager::new());
    }
    NGX_OK as ngx_int_t
}

extern "C" fn vts_init_process(cycle: *mut ngx_cycle_t) -> ngx_int_t {
    NGX_OK as ngx_int_t
}

// Configuration handlers
extern "C" fn vts_set_status(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let loc_conf = conf as *mut VtsConfig;
    unsafe {
        (*loc_conf).enable_status = true;
        
        // Set up the handler for this location
        let clcf = ngx_http_conf_get_module_loc_conf(cf, &ngx_http_core_module as *const _ as *mut _) as *mut ngx_http_core_loc_conf_t;
        if !clcf.is_null() {
            (*clcf).handler = Some(VtsHandler::vts_status_handler);
        }
    }
    std::ptr::null_mut()
}

// Post-configuration hook to set up request tracking
extern "C" fn vts_postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
    unsafe {
        let cmcf = ngx_http_conf_get_module_main_conf(cf, &ngx_http_core_module as *const _ as *mut _) as *mut ngx_http_core_main_conf_t;
        if cmcf.is_null() {
            return NGX_ERROR as ngx_int_t;
        }

        // Register log phase handler for request tracking
        let h = ngx_array_push(&mut (*cmcf).phases[NGX_HTTP_LOG_PHASE as usize].handlers) as *mut ngx_http_handler_pt;
        if h.is_null() {
            return NGX_ERROR as ngx_int_t;
        }
        *h = Some(vts_log_handler);
    }
    NGX_OK as ngx_int_t
}

// Log phase handler for collecting request statistics
extern "C" fn vts_log_handler(r: *mut ngx_http_request_t) -> ngx_int_t {
    unsafe {
        // Check if VTS is enabled for this location
        let loc_conf = ngx_http_get_module_loc_conf(r, &ngx_http_vts_module as *const _ as *mut _) as *mut VtsConfig;
        if loc_conf.is_null() || !(*loc_conf).enable_zone {
            return NGX_DECLINED as ngx_int_t;
        }

        if let Some(ref manager) = VTS_MANAGER {
            // Extract server name
            let server_name = if !(*r).headers_in.server.data.is_null() && (*r).headers_in.server.len > 0 {
                std::str::from_utf8_unchecked(std::slice::from_raw_parts(
                    (*r).headers_in.server.data,
                    (*r).headers_in.server.len,
                ))
            } else {
                "_"  // Default server
            };

            // Get request statistics
            let status = (*r).headers_out.status;
            let bytes_in = (*r).request_length as u64;
            let bytes_out = (*r).connection.sent as u64;
            
            // Calculate request time (approximate)
            let request_time = if (*r).start_sec > 0 {
                let current = ngx_time();
                let elapsed_sec = current - (*r).start_sec;
                let elapsed_msec = ngx_current_msec - (*r).start_msec;
                elapsed_sec as f64 + (elapsed_msec as f64 / 1000.0)
            } else {
                0.0
            };

            // Update statistics
            manager.update_request_stats(server_name, status as u16, bytes_in, bytes_out, request_time);
        }
    }
    NGX_OK as ngx_int_t
}

extern "C" fn vts_set_status_zone(
    cf: *mut ngx_conf_t,
    cmd: *mut ngx_command_t,
    conf: *mut c_void,
) -> *mut c_char {
    let config = conf as *mut VtsConfig;
    unsafe {
        let args = (*(*cf).args).elts as *mut ngx_str_t;
        let value = *args.offset(1);
        
        if ngx::str::str_eq(&value, b"on") {
            (*config).enable_zone = true;
        } else if ngx::str::str_eq(&value, b"off") {
            (*config).enable_zone = false;
        }
    }
    std::ptr::null_mut()
}

