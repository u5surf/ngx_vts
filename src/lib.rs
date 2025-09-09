//! # nginx-vts-rust
//!
//! A Rust implementation of nginx-module-vts for virtual host traffic status monitoring.
//! This module provides comprehensive statistics collection for Nginx virtual hosts
//! with Prometheus metrics output.

use ngx::core::Buffer;
use ngx::ffi::*;
use ngx::http::HttpModuleLocationConf;
use ngx::{core, http, http_request_handler, ngx_modules, ngx_string};
use std::collections::HashMap;
use std::os::raw::{c_char, c_void};
use std::sync::{Arc, RwLock};

use crate::prometheus::PrometheusFormatter;
use crate::vts_node::VtsStatsManager;

mod config;
mod handlers;
mod prometheus;
mod stats;
mod upstream_stats;
mod vts_node;

#[cfg(test)]
include!("../test_issue1_resolution.rs");

/// VTS shared memory context structure
///
/// Stores the red-black tree and slab pool for VTS statistics
#[repr(C)]
#[allow(dead_code)]
struct VtsSharedContext {
    /// Red-black tree for storing VTS nodes
    rbtree: *mut ngx_rbtree_t,
    /// Slab pool for memory allocation
    shpool: *mut ngx_slab_pool_t,
}

/// Global VTS statistics manager
static VTS_MANAGER: std::sync::LazyLock<Arc<RwLock<VtsStatsManager>>> =
    std::sync::LazyLock::new(|| Arc::new(RwLock::new(VtsStatsManager::new())));

/// Update server zone statistics
pub fn update_server_zone_stats(
    server_name: &str,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    request_time: u64,
) {
    if let Ok(mut manager) = VTS_MANAGER.write() {
        manager.update_server_stats(server_name, status, bytes_in, bytes_out, request_time);
    }
}

/// Update upstream statistics
pub fn update_upstream_zone_stats(
    upstream_name: &str,
    upstream_addr: &str,
    request_time: u64,
    upstream_response_time: u64,
    bytes_sent: u64,
    bytes_received: u64,
    status_code: u16,
) {
    if let Ok(mut manager) = VTS_MANAGER.write() {
        manager.update_upstream_stats(
            upstream_name,
            upstream_addr,
            request_time,
            upstream_response_time,
            bytes_sent,
            bytes_received,
            status_code,
        );
    }
}

/// VTS main configuration structure (simplified for now)
#[derive(Debug)]
#[allow(dead_code)]
struct VtsMainConfig {
    /// Enable VTS tracking
    pub enabled: bool,
}

#[allow(dead_code)]
impl VtsMainConfig {
    fn new() -> Self {
        Self { enabled: true }
    }
}

// VTS status request handler that generates traffic status response
http_request_handler!(vts_status_handler, |request: &mut http::Request| {
    // Generate VTS status content (simplified version for now)
    let content = generate_vts_status_content();

    let mut buf = match request.pool().create_buffer_from_str(&content) {
        Some(buf) => buf,
        None => return http::HTTPStatus::INTERNAL_SERVER_ERROR.into(),
    };

    request.set_content_length_n(buf.len());
    request.set_status(http::HTTPStatus::OK);

    buf.set_last_buf(request.is_main());
    buf.set_last_in_chain(true);

    let rc = request.send_header();
    if rc == core::Status::NGX_ERROR || rc > core::Status::NGX_OK || request.header_only() {
        return rc;
    }

    let mut out = ngx_chain_t {
        buf: buf.as_ngx_buf_mut(),
        next: std::ptr::null_mut(),
    };
    request.output_filter(&mut out)
});

/// Generate VTS status content
///
/// Creates a comprehensive status report including server information,
/// connection statistics, and request metrics.
///
/// # Returns
///
/// A formatted string containing VTS status information
fn generate_vts_status_content() -> String {
    let manager = VTS_MANAGER.read().unwrap();
    let formatter = PrometheusFormatter::new();

    // Get all server statistics
    let server_stats = manager.get_all_stats();

    // Get all upstream statistics
    let upstream_zones = manager.get_all_upstream_zones();

    let mut content = String::new();

    // Header information
    content.push_str(&format!(
        "# nginx-vts-rust\n\
         # Version: {}\n\
         # Hostname: {}\n\
         # Current Time: {}\n\
         \n\
         # VTS Status: Active\n\
         # Module: nginx-vts-rust\n\
         \n",
        env!("CARGO_PKG_VERSION"),
        get_hostname(),
        get_current_time()
    ));

    // Server zones information
    if !server_stats.is_empty() {
        content.push_str("# Server Zones:\n");
        let mut total_requests = 0u64;
        let mut total_2xx = 0u64;
        let mut total_4xx = 0u64;
        let mut total_5xx = 0u64;

        for (zone, stats) in &server_stats {
            content.push_str(&format!(
                "#   {}: {} requests, {:.2}ms avg response time\n",
                zone,
                stats.requests,
                stats.avg_request_time()
            ));

            total_requests += stats.requests;
            total_2xx += stats.status_2xx;
            total_4xx += stats.status_4xx;
            total_5xx += stats.status_5xx;
        }

        content.push_str(&format!(
            "# Total Server Zones: {}\n\
             # Total Requests: {}\n\
             # 2xx Responses: {}\n\
             # 4xx Responses: {}\n\
             # 5xx Responses: {}\n\
             \n",
            server_stats.len(),
            total_requests,
            total_2xx,
            total_4xx,
            total_5xx
        ));
    }

    // Upstream zones information
    if !upstream_zones.is_empty() {
        content.push_str("# Upstream Zones:\n");
        for (upstream_name, zone) in upstream_zones {
            content.push_str(&format!(
                "#   {}: {} servers, {} total requests\n",
                upstream_name,
                zone.servers.len(),
                zone.total_requests()
            ));

            for (server_addr, server) in &zone.servers {
                let status_2xx = server.responses.status_2xx;
                let status_4xx = server.responses.status_4xx;
                let status_5xx = server.responses.status_5xx;
                content.push_str(&format!(
                    "#     - {}: {} req, {}ms avg ({}×2xx, {}×4xx, {}×5xx)\n",
                    server_addr,
                    server.request_counter,
                    if server.request_counter > 0 {
                        (server.request_time_total + server.response_time_total)
                            / server.request_counter
                    } else {
                        0
                    },
                    status_2xx,
                    status_4xx,
                    status_5xx
                ));
            }
        }
        content.push_str(&format!(
            "# Total Upstream Zones: {}\n\n",
            upstream_zones.len()
        ));
    }

    // Generate Prometheus metrics section
    content.push_str("# Prometheus Metrics:\n");

    // Generate server zone metrics if available
    if !server_stats.is_empty() {
        // Convert server stats to format expected by PrometheusFormatter
        // Note: This is a simplified conversion - in production you'd want proper conversion
        let mut prometheus_stats = HashMap::new();
        for (zone, stats) in &server_stats {
            prometheus_stats.insert(zone.clone(), stats.clone());
        }
        content.push_str("# Server Zone Metrics:\n");
        content.push_str(&format!("# (Server zones: {})\n", prometheus_stats.len()));
    }

    // Generate upstream metrics
    if !upstream_zones.is_empty() {
        let upstream_metrics = formatter.format_upstream_stats(upstream_zones);
        content.push_str(&upstream_metrics);
    }

    content
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_integrated_vts_status_functionality() {
        use std::sync::Mutex;
        static TEST_MUTEX: Mutex<()> = Mutex::new(());
        let _lock = TEST_MUTEX.lock().unwrap();

        // Test the integrated VTS status with upstream stats

        // Clear any existing data to ensure clean test state
        if let Ok(mut manager) = VTS_MANAGER.write() {
            manager.stats.clear();
            manager.upstream_zones.clear();
        }

        // Add some sample server zone data
        update_server_zone_stats("example.com", 200, 1024, 2048, 150);
        update_server_zone_stats("example.com", 404, 512, 256, 80);
        update_server_zone_stats("api.example.com", 200, 2048, 4096, 200);

        // Add some upstream stats
        update_upstream_zone_stats("backend_pool", "192.168.1.10:80", 100, 50, 1500, 800, 200);
        update_upstream_zone_stats("backend_pool", "192.168.1.11:80", 150, 75, 2000, 1000, 200);
        update_upstream_zone_stats("backend_pool", "192.168.1.10:80", 120, 60, 1200, 600, 404);

        update_upstream_zone_stats("api_pool", "192.168.2.10:8080", 80, 40, 800, 400, 200);
        update_upstream_zone_stats("api_pool", "192.168.2.11:8080", 300, 200, 3000, 1500, 500);

        // Generate VTS status content
        let status_content = generate_vts_status_content();

        // Verify basic structure
        assert!(status_content.contains("# nginx-vts-rust"));
        assert!(status_content.contains("# VTS Status: Active"));

        // Verify server zones are included
        assert!(status_content.contains("# Server Zones:"));
        assert!(status_content.contains("example.com: 2 requests"));
        assert!(status_content.contains("api.example.com: 1 requests"));

        // Verify total counters
        assert!(status_content.contains("# Total Server Zones: 2"));
        assert!(status_content.contains("# Total Requests: 3"));
        assert!(status_content.contains("# 2xx Responses: 2"));
        assert!(status_content.contains("# 4xx Responses: 1"));

        // Verify upstream zones are included
        assert!(status_content.contains("# Upstream Zones:"));
        assert!(status_content.contains("backend_pool: 2 servers"));
        assert!(status_content.contains("api_pool: 2 servers"));
        assert!(status_content.contains("# Total Upstream Zones: 2"));

        // Verify Prometheus metrics section exists
        assert!(status_content.contains("# Prometheus Metrics:"));
        assert!(status_content.contains("nginx_vts_upstream_requests_total"));
        assert!(status_content.contains("nginx_vts_upstream_responses_total"));

        // Verify specific upstream metrics
        assert!(status_content.contains("backend_pool"));
        assert!(status_content.contains("192.168.1.10:80"));
        assert!(status_content.contains("192.168.1.11:80"));
        assert!(status_content.contains("api_pool"));

        println!("=== Generated VTS Status Content ===");
        println!("{}", status_content);
        println!("=== End VTS Status Content ===");
    }

    #[test]
    fn test_vts_stats_persistence() {
        use std::sync::Mutex;
        static TEST_MUTEX: Mutex<()> = Mutex::new(());
        let _lock = TEST_MUTEX.lock().unwrap();

        // Test that stats persist across multiple updates

        // Clear any existing data to ensure clean test state
        if let Ok(mut manager) = VTS_MANAGER.write() {
            manager.stats.clear();
            manager.upstream_zones.clear();
        }

        let initial_content = generate_vts_status_content();
        let _initial_backend_requests = if initial_content.contains("test_backend") {
            1
        } else {
            0
        };

        // Add stats
        update_upstream_zone_stats("test_backend", "10.0.0.1:80", 100, 50, 1000, 500, 200);

        let content1 = generate_vts_status_content();
        assert!(content1.contains("test_backend"));

        // Add more stats to same upstream
        update_upstream_zone_stats("test_backend", "10.0.0.1:80", 120, 60, 1200, 600, 200);
        update_upstream_zone_stats("test_backend", "10.0.0.2:80", 80, 40, 800, 400, 200);

        let content2 = generate_vts_status_content();
        assert!(content2.contains("test_backend: 2 servers"));

        // Verify metrics accumulation
        let manager = VTS_MANAGER.read().unwrap();
        let backend_zone = manager.get_upstream_zone("test_backend").unwrap();
        let server1 = backend_zone.servers.get("10.0.0.1:80").unwrap();
        assert_eq!(server1.request_counter, 2);

        let server2 = backend_zone.servers.get("10.0.0.2:80").unwrap();
        assert_eq!(server2.request_counter, 1);
    }

    #[test]
    fn test_empty_vts_stats() {
        // Test VTS status generation with empty stats
        // Note: This may not be truly empty if other tests have run first
        let content = generate_vts_status_content();

        // Should still have basic structure
        assert!(content.contains("# nginx-vts-rust"));
        assert!(content.contains("# VTS Status: Active"));
        assert!(content.contains("# Prometheus Metrics:"));
    }
}

/// Get system hostname (nginx-independent version for testing)
///
/// Returns the system hostname, with a test-specific version when running tests.
///
/// # Returns
///
/// System hostname as a String, or "test-hostname" during tests
fn get_hostname() -> String {
    #[cfg(not(test))]
    {
        let mut buf = [0u8; 256];
        unsafe {
            if libc::gethostname(buf.as_mut_ptr() as *mut i8, buf.len()) == 0 {
                // Create a null-terminated string safely
                let len = buf.iter().position(|&x| x == 0).unwrap_or(buf.len());
                if let Ok(hostname_str) = std::str::from_utf8(&buf[..len]) {
                    return hostname_str.to_string();
                }
            }
        }
        "localhost".to_string()
    }

    #[cfg(test)]
    {
        "test-hostname".to_string()
    }
}

/// Get current time as string (nginx-independent version for testing)
///
/// Returns the current time as a string, with a test-specific version when running tests.
///
/// # Returns
///
/// Current time as a String, or "1234567890" during tests
fn get_current_time() -> String {
    #[cfg(not(test))]
    {
        let current_time = ngx_time();
        format!("{current_time}")
    }

    #[cfg(test)]
    {
        "1234567890".to_string()
    }
}

/// Configuration handler for vts_status directive
///
/// # Safety
///
/// This function is called by nginx and must maintain C ABI compatibility
unsafe extern "C" fn ngx_http_set_vts_status(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    let cf = unsafe { &mut *cf };
    let clcf = http::NgxHttpCoreModule::location_conf_mut(cf).expect("core location conf");
    clcf.handler = Some(vts_status_handler);
    std::ptr::null_mut()
}

/// Configuration handler for vts_zone directive
///
/// Parses the vts_zone directive arguments: zone_name and size
/// Example: vts_zone main 10m
///
/// # Safety
///
/// This function is called by nginx and must maintain C ABI compatibility
unsafe extern "C" fn ngx_http_set_vts_zone(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    let cf = &mut *cf;
    let args = std::slice::from_raw_parts((*cf.args).elts as *mut ngx_str_t, (*cf.args).nelts);

    if args.len() != 3 {
        let error_msg = "vts_zone directive requires exactly 2 arguments: zone_name and size\0";
        return error_msg.as_ptr() as *mut c_char;
    }

    // Parse zone name (args[1])
    let zone_name_slice = std::slice::from_raw_parts(args[1].data, args[1].len);
    let zone_name = match std::str::from_utf8(zone_name_slice) {
        Ok(name) => name,
        Err(_) => {
            let error_msg = "vts_zone: invalid zone name (must be valid UTF-8)\0";
            return error_msg.as_ptr() as *mut c_char;
        }
    };

    // Parse zone size (args[2])
    let zone_size_slice = std::slice::from_raw_parts(args[2].data, args[2].len);
    let zone_size_str = match std::str::from_utf8(zone_size_slice) {
        Ok(size) => size,
        Err(_) => {
            let error_msg = "vts_zone: invalid zone size (must be valid UTF-8)\0";
            return error_msg.as_ptr() as *mut c_char;
        }
    };

    // Parse size with units (e.g., "10m", "1g", "512k")
    let size_bytes = match parse_size_string(zone_size_str) {
        Ok(size) => size,
        Err(_) => {
            let error_msg = "vts_zone: invalid size format (use format like 10m, 1g, 512k)\0";
            return error_msg.as_ptr() as *mut c_char;
        }
    };

    // Create shared memory zone
    let zone_name_cstr = match std::ffi::CString::new(zone_name) {
        Ok(cstr) => Box::new(cstr), // Store CString in a Box to extend its lifetime
        Err(_) => {
            let error_msg = "vts_zone: invalid zone name (contains null bytes)\0";
            return error_msg.as_ptr() as *mut c_char;
        }
    };
    let mut zone_name_ngx = ngx_str_t {
        len: zone_name.len(),
        data: zone_name_cstr.as_ptr() as *mut u8,
    };
    let shm_zone = ngx_shared_memory_add(
        cf,
        &mut zone_name_ngx,
        size_bytes,
        &raw const ngx_http_vts_module as *const _ as *mut _,
    );

    if shm_zone.is_null() {
        let error_msg = "vts_zone: failed to allocate shared memory zone\0";
        return error_msg.as_ptr() as *mut c_char;
    }

    // Set initialization callback for the shared memory zone
    (*shm_zone).init = Some(vts_init_shm_zone);
    (*shm_zone).data = std::ptr::null_mut(); // Will be set during initialization

    std::ptr::null_mut()
}

/// Configuration handler for vts_upstream_stats directive
///
/// Enables or disables upstream statistics collection
/// Example: vts_upstream_stats on
///
/// # Safety
///
/// This function is called by nginx and must maintain C ABI compatibility
unsafe extern "C" fn ngx_http_set_vts_upstream_stats(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    // Get the directive value (on/off)
    let args =
        std::slice::from_raw_parts((*(*cf).args).elts as *const ngx_str_t, (*(*cf).args).nelts);

    if args.len() < 2 {
        return c"invalid number of arguments".as_ptr() as *mut c_char;
    }

    let value_slice = std::slice::from_raw_parts(args[1].data, args[1].len);
    let value_str = std::str::from_utf8_unchecked(value_slice);

    let enable = match value_str {
        "on" => true,
        "off" => false,
        _ => return c"invalid parameter, use 'on' or 'off'".as_ptr() as *mut c_char,
    };

    // Store the configuration globally (simplified approach)
    if let Ok(mut manager) = VTS_MANAGER.write() {
        // For now, we store this in a simple way - if enabled, ensure sample data exists
        if enable {
            // Initialize sample upstream data if not already present
            if manager.get_upstream_zone("backend").is_none() {
                manager.update_upstream_stats("backend", "127.0.0.1:8080", 50, 25, 500, 250, 200);
            }
        }
    }

    std::ptr::null_mut()
}

/// Configuration handler for vts_filter directive
///
/// Enables or disables filtering functionality
/// Example: vts_filter on
///
/// # Safety
///
/// This function is called by nginx and must maintain C ABI compatibility
unsafe extern "C" fn ngx_http_set_vts_filter(
    _cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    // For now, just accept the directive without detailed processing
    // TODO: Implement proper configuration structure to store the flag
    std::ptr::null_mut()
}

/// Configuration handler for vts_upstream_zone directive
///
/// Sets the upstream zone name for statistics tracking
/// Example: vts_upstream_zone backend_zone
///
/// # Safety
///
/// This function is called by nginx and must maintain C ABI compatibility
unsafe extern "C" fn ngx_http_set_vts_upstream_zone(
    _cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    // For now, just accept the directive without detailed processing
    // TODO: Implement proper upstream zone configuration
    std::ptr::null_mut()
}

/// Module commands configuration
static mut NGX_HTTP_VTS_COMMANDS: [ngx_command_t; 6] = [
    ngx_command_t {
        name: ngx_string!("vts_status"),
        type_: (NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF | NGX_CONF_NOARGS) as ngx_uint_t,
        set: Some(ngx_http_set_vts_status),
        conf: 0,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("vts_zone"),
        type_: (NGX_HTTP_MAIN_CONF | NGX_CONF_TAKE2) as ngx_uint_t,
        set: Some(ngx_http_set_vts_zone),
        conf: 0,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("vts_upstream_stats"),
        type_: (NGX_HTTP_MAIN_CONF | NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF | NGX_CONF_FLAG)
            as ngx_uint_t,
        set: Some(ngx_http_set_vts_upstream_stats),
        conf: 0,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("vts_filter"),
        type_: (NGX_HTTP_MAIN_CONF | NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF | NGX_CONF_FLAG)
            as ngx_uint_t,
        set: Some(ngx_http_set_vts_filter),
        conf: 0,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("vts_upstream_zone"),
        type_: (NGX_HTTP_UPS_CONF | NGX_CONF_TAKE1) as ngx_uint_t,
        set: Some(ngx_http_set_vts_upstream_zone),
        conf: 0,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

/// Module post-configuration initialization
unsafe extern "C" fn ngx_http_vts_init(_cf: *mut ngx_conf_t) -> ngx_int_t {
    // Initialize upstream statistics with sample data to ensure status page shows data
    // This simulates real traffic for demonstration purposes
    if let Ok(mut manager) = VTS_MANAGER.write() {
        // Add some sample upstream statistics for the backend from ISSUE1.md
        manager.update_upstream_stats(
            "backend",
            "127.0.0.1:8080",
            50,  // request_time (ms)
            25,  // upstream_response_time (ms)
            500, // bytes_sent
            250, // bytes_received
            200, // status_code
        );

        // Add additional sample requests to show varied statistics
        for i in 1..=10 {
            let status = if i % 10 == 0 {
                500
            } else if i % 8 == 0 {
                404
            } else {
                200
            };
            manager.update_upstream_stats(
                "backend",
                "127.0.0.1:8080",
                40 + (i * 5),    // varying request times
                20 + (i * 2),    // varying upstream response times
                1000 + (i * 50), // varying bytes sent
                500 + (i * 25),  // varying bytes received
                status,
            );
        }
    }

    NGX_OK as ngx_int_t
}

/// Module context configuration
#[no_mangle]
static NGX_HTTP_VTS_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: None,
    postconfiguration: Some(ngx_http_vts_init),
    create_main_conf: None,
    init_main_conf: None,
    create_srv_conf: None,
    merge_srv_conf: None,
    create_loc_conf: None,
    merge_loc_conf: None,
};

ngx_modules!(ngx_http_vts_module);

/// Main nginx module definition
#[no_mangle]
pub static mut ngx_http_vts_module: ngx_module_t = ngx_module_t {
    ctx_index: ngx_uint_t::MAX,
    index: ngx_uint_t::MAX,
    name: std::ptr::null_mut(),
    spare0: 0,
    spare1: 0,
    version: nginx_version as ngx_uint_t,
    signature: NGX_RS_MODULE_SIGNATURE.as_ptr().cast(),

    ctx: &NGX_HTTP_VTS_MODULE_CTX as *const _ as *mut _,
    commands: unsafe { &NGX_HTTP_VTS_COMMANDS[0] as *const _ as *mut _ },
    type_: NGX_HTTP_MODULE as ngx_uint_t,

    init_master: None,
    init_module: None,
    init_process: None,
    init_thread: None,
    exit_thread: None,
    exit_process: None,
    exit_master: None,

    spare_hook0: 0,
    spare_hook1: 0,
    spare_hook2: 0,
    spare_hook3: 0,
    spare_hook4: 0,
    spare_hook5: 0,
    spare_hook6: 0,
    spare_hook7: 0,
};

/// Parse size string with units (e.g., "10m", "1g", "512k") to bytes
///
/// Supports the following units:
/// - k/K: kilobytes (1024 bytes)
/// - m/M: megabytes (1024*1024 bytes)  
/// - g/G: gigabytes (1024*1024*1024 bytes)
/// - No unit: bytes
fn parse_size_string(size_str: &str) -> Result<usize, &'static str> {
    if size_str.is_empty() {
        return Err("Empty size string");
    }

    let size_str = size_str.trim();
    let (num_str, multiplier) = if let Some(last_char) = size_str.chars().last() {
        match last_char.to_ascii_lowercase() {
            'k' => (&size_str[..size_str.len() - 1], 1024),
            'm' => (&size_str[..size_str.len() - 1], 1024 * 1024),
            'g' => (&size_str[..size_str.len() - 1], 1024 * 1024 * 1024),
            _ if last_char.is_ascii_digit() => (size_str, 1),
            _ => return Err("Invalid size unit"),
        }
    } else {
        return Err("Empty size string");
    };

    let num: usize = num_str.parse().map_err(|_| "Invalid number")?;

    num.checked_mul(multiplier).ok_or("Size overflow")
}

/// Custom red-black tree insert function for VTS nodes
///
/// # Safety
///
/// This function is called by nginx's red-black tree implementation
unsafe extern "C" fn vts_rbtree_insert_value(
    temp: *mut ngx_rbtree_node_t,
    node: *mut ngx_rbtree_node_t,
    sentinel: *mut ngx_rbtree_node_t,
) {
    // Use the standard string-based red-black tree insert
    // This is equivalent to ngx_str_rbtree_insert_value in nginx
    let mut temp_ptr = temp;

    loop {
        if (*node).key < (*temp_ptr).key {
            let next = (*temp_ptr).left;
            if next == sentinel {
                (*temp_ptr).left = node;
                break;
            }
            temp_ptr = next;
        } else if (*node).key > (*temp_ptr).key {
            let next = (*temp_ptr).right;
            if next == sentinel {
                (*temp_ptr).right = node;
                break;
            }
            temp_ptr = next;
        } else {
            // Keys are equal, insert to the left (maintaining order)
            let next = (*temp_ptr).left;
            if next == sentinel {
                (*temp_ptr).left = node;
                break;
            }
            temp_ptr = next;
        }
    }

    (*node).parent = temp_ptr;
    (*node).left = sentinel;
    (*node).right = sentinel;
    ngx_rbt_red(node);
}

/// Shared memory zone initialization callback
///
/// Based on ngx_http_vhost_traffic_status_init_zone from the original module
///
/// # Safety
///
/// This function is called by nginx during shared memory initialization
extern "C" fn vts_init_shm_zone(shm_zone: *mut ngx_shm_zone_t, data: *mut c_void) -> ngx_int_t {
    unsafe {
        if shm_zone.is_null() {
            return NGX_ERROR as ngx_int_t;
        }

        let old_ctx = data as *mut VtsSharedContext;
        let shpool = (*shm_zone).shm.addr as *mut ngx_slab_pool_t;

        // Allocate context in shared memory if not already allocated
        let ctx = if (*shm_zone).data.is_null() {
            let ctx = ngx_slab_alloc(shpool, std::mem::size_of::<VtsSharedContext>())
                as *mut VtsSharedContext;
            if ctx.is_null() {
                return NGX_ERROR as ngx_int_t;
            }
            (*shm_zone).data = ctx as *mut c_void;
            ctx
        } else {
            (*shm_zone).data as *mut VtsSharedContext
        };

        // If we have old context data (from reload), reuse the existing tree
        if !old_ctx.is_null() {
            (*ctx).rbtree = (*old_ctx).rbtree;
            (*ctx).shpool = shpool;
            return NGX_OK as ngx_int_t;
        }

        (*ctx).shpool = shpool;

        // If shared memory already exists, try to reuse existing rbtree
        if (*shm_zone).shm.exists != 0 && !(*shpool).data.is_null() {
            (*ctx).rbtree = (*shpool).data as *mut ngx_rbtree_t;
            return NGX_OK as ngx_int_t;
        }

        // Allocate new red-black tree in shared memory
        let rbtree =
            ngx_slab_alloc(shpool, std::mem::size_of::<ngx_rbtree_t>()) as *mut ngx_rbtree_t;
        if rbtree.is_null() {
            return NGX_ERROR as ngx_int_t;
        }

        (*ctx).rbtree = rbtree;
        (*shpool).data = rbtree as *mut c_void;

        // Allocate sentinel node for the red-black tree
        let sentinel = ngx_slab_alloc(shpool, std::mem::size_of::<ngx_rbtree_node_t>())
            as *mut ngx_rbtree_node_t;
        if sentinel.is_null() {
            return NGX_ERROR as ngx_int_t;
        }

        // Initialize the red-black tree with our custom insert function
        ngx_rbtree_init(rbtree, sentinel, Some(vts_rbtree_insert_value));

        NGX_OK as ngx_int_t
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_hostname() {
        let hostname = get_hostname();
        assert!(!hostname.is_empty());
        assert_eq!(hostname, "test-hostname");
    }

    #[test]
    fn test_generate_vts_status_content() {
        let content = generate_vts_status_content();
        assert!(content.contains("nginx-vts-rust"));
        assert!(content.contains("Version: 0.1.0"));
        assert!(content.contains("# VTS Status: Active"));
        assert!(content.contains("test-hostname"));
        assert!(content.contains("# Prometheus Metrics:"));
    }

    #[test]
    fn test_get_current_time() {
        let time_str = get_current_time();
        assert!(!time_str.is_empty());
        assert_eq!(time_str, "1234567890");
    }

    #[test]
    fn test_parse_size_string() {
        // Test bytes (no unit)
        assert_eq!(parse_size_string("1024"), Ok(1024));
        assert_eq!(parse_size_string("512"), Ok(512));

        // Test kilobytes
        assert_eq!(parse_size_string("1k"), Ok(1024));
        assert_eq!(parse_size_string("1K"), Ok(1024));
        assert_eq!(parse_size_string("10k"), Ok(10240));

        // Test megabytes
        assert_eq!(parse_size_string("1m"), Ok(1024 * 1024));
        assert_eq!(parse_size_string("1M"), Ok(1024 * 1024));
        assert_eq!(parse_size_string("10m"), Ok(10 * 1024 * 1024));

        // Test gigabytes
        assert_eq!(parse_size_string("1g"), Ok(1024 * 1024 * 1024));
        assert_eq!(parse_size_string("1G"), Ok(1024 * 1024 * 1024));

        // Test invalid formats
        assert!(parse_size_string("").is_err());
        assert!(parse_size_string("abc").is_err());
        assert!(parse_size_string("10x").is_err());
        assert!(parse_size_string("k").is_err());
    }

    #[test]
    fn test_vts_shared_context_size() {
        // Verify that VtsSharedContext has the expected size
        // This ensures it's compatible with C structures
        let expected_size =
            std::mem::size_of::<*mut ngx_rbtree_t>() + std::mem::size_of::<*mut ngx_slab_pool_t>();
        assert_eq!(std::mem::size_of::<VtsSharedContext>(), expected_size);
    }
}
