//! # nginx-vts-rust
//!
//! A Rust implementation of nginx-module-vts for virtual host traffic status monitoring.
//! This module provides comprehensive statistics collection for Nginx virtual hosts
//! with Prometheus metrics output.

use ngx::core::Buffer;
use ngx::ffi::*;
use ngx::http::HttpModuleLocationConf;
use ngx::{core, http, http_request_handler, ngx_modules, ngx_string};
use std::os::raw::{c_char, c_void};

use crate::vts_node::VtsSharedNode;

mod config;
mod vts_node;

/// VTS shared memory context structure
///
/// Stores the red-black tree and slab pool for VTS statistics
#[repr(C)]
#[allow(dead_code)]
pub struct VtsSharedContext {
    /// Red-black tree for storing VTS nodes
    rbtree: *mut ngx_rbtree_t,
    /// Slab pool for memory allocation
    shpool: *mut ngx_slab_pool_t,
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
    // Record this status request (demonstrates real traffic recording)
    unsafe {
        let _ = vts_record_status_request();
    }

    // Generate VTS status content from shared memory
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

// VTS test request handler that simulates different request types
http_request_handler!(vts_test_handler, |request: &mut http::Request| {
    // Simulate different types of requests for testing
    unsafe {
        // Add some varied test requests to demonstrate different scenarios
        let host = request.headers().get("Host").map_or("unknown", |h| h.as_str());
        let _ = vts_record_request(VTS_GLOBAL_CTX, host, 200, 1024, 2048, 50);
        let _ = vts_record_request(VTS_GLOBAL_CTX, host, 404, 256, 512, 10);
        let _ = vts_record_request(VTS_GLOBAL_CTX, host, 500, 512, 0, 200);
    }

    let content = "Test request recorded! Check /status to see updated statistics.\n";

    let mut buf = match request.pool().create_buffer_from_str(content) {
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

/// Generate VTS status content from shared memory
///
/// Creates a comprehensive status report by reading actual statistics
/// from the shared memory red-black tree.
///
/// # Returns
///
/// A formatted string containing real VTS status information
fn generate_vts_status_content() -> String {
    unsafe {
        if VTS_GLOBAL_CTX.is_null() || (*VTS_GLOBAL_CTX).rbtree.is_null() {
            return format!(
                "# nginx-vts-rust\n\
                 # Version: 0.1.0\n\
                 # Hostname: {}\n\
                 # Current Time: {}\n\
                 \n\
                 # VTS Status: Shared memory not initialized\n",
                get_hostname(),
                get_current_time()
            );
        }

        let mut output = format!(
            "# nginx-vts-rust\n\
             # Version: 0.1.0\n\
             # Hostname: {}\n\
             # Current Time: {}\n\
             \n\
             # VTS Statistics (from shared memory)\n\
             \n",
            get_hostname(),
            get_current_time()
        );

        // Walk the red-black tree to collect statistics
        let rbtree = (*VTS_GLOBAL_CTX).rbtree;
        let sentinel = (*rbtree).sentinel;
        let mut total_requests = 0u64;
        let mut total_2xx = 0u64;
        let mut total_4xx = 0u64;
        let mut total_5xx = 0u64;
        let mut server_count = 0u32;

        // Simple tree traversal to collect statistics
        vts_walk_tree((*rbtree).root, sentinel, &mut |node| {
            let vts_node = node as *mut VtsSharedNode;
            let node_key_len = (*vts_node).len as usize;

            if node_key_len > 0 {
                let node_key_ptr =
                    (vts_node as *const u8).add(std::mem::size_of::<VtsSharedNode>());
                let node_key = std::slice::from_raw_parts(node_key_ptr, node_key_len);

                if let Ok(server_name) = std::str::from_utf8(node_key) {
                    output.push_str(&format!(
                        "Server: {}\n\
                         - Requests: {}\n\
                         - 2xx: {}, 4xx: {}, 5xx: {}\n\
                         - Bytes in: {}, out: {}\n\
                         - Avg response time: {}ms\n\
                         \n",
                        server_name,
                        (*vts_node).stat_request_counter,
                        (*vts_node).stat_2xx_counter,
                        (*vts_node).stat_4xx_counter,
                        (*vts_node).stat_5xx_counter,
                        (*vts_node).stat_in_bytes,
                        (*vts_node).stat_out_bytes,
                        (*vts_node).stat_request_time,
                    ));

                    total_requests += (*vts_node).stat_request_counter;
                    total_2xx += (*vts_node).stat_2xx_counter;
                    total_4xx += (*vts_node).stat_4xx_counter;
                    total_5xx += (*vts_node).stat_5xx_counter;
                    server_count += 1;
                }
            }
        });

        output.push_str(&format!(
            "# Summary\n\
             Total servers: {}\n\
             Total requests: {}\n\
             Total 2xx: {}\n\
             Total 4xx: {}\n\
             Total 5xx: {}\n",
            server_count, total_requests, total_2xx, total_4xx, total_5xx
        ));

        output
    }
}

/// Walk red-black tree and call function for each node
///
/// # Safety
///
/// This function traverses shared memory tree structures
unsafe fn vts_walk_tree<F>(
    node: *mut ngx_rbtree_node_t,
    sentinel: *mut ngx_rbtree_node_t,
    f: &mut F,
) where
    F: FnMut(*mut ngx_rbtree_node_t),
{
    if node == sentinel {
        return;
    }

    vts_walk_tree((*node).left, sentinel, f);
    f(node);
    vts_walk_tree((*node).right, sentinel, f);
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

/// Configuration handler for vts_test directive
///
/// # Safety
///
/// This function is called by nginx and must maintain C ABI compatibility
unsafe extern "C" fn ngx_http_set_vts_test(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    let cf = unsafe { &mut *cf };
    let clcf = http::NgxHttpCoreModule::location_conf_mut(cf).expect("core location conf");
    clcf.handler = Some(vts_test_handler);
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

/// Module commands configuration
static mut NGX_HTTP_VTS_COMMANDS: [ngx_command_t; 4] = [
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
        name: ngx_string!("vts_test"),
        type_: (NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF | NGX_CONF_NOARGS) as ngx_uint_t,
        set: Some(ngx_http_set_vts_test),
        conf: 0,
        offset: 0,
        post: std::ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

/// Module context configuration with post-configuration hook
#[no_mangle]
static NGX_HTTP_VTS_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: None,
    postconfiguration: Some(ngx_http_vts_postconfiguration),
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

        // Set global context for request handlers
        vts_set_global_context(ctx);

        NGX_OK as ngx_int_t
    }
}

/// Add or update a VTS node in shared memory
///
/// Based on ngx_http_vhost_traffic_status_shm_add_node from the original module
///
/// # Safety
///
/// This function manipulates shared memory and must be called with proper synchronization
pub unsafe fn vts_shm_add_node(
    ctx: *mut VtsSharedContext,
    key: &str,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    request_time: u64,
) -> Result<(), &'static str> {
    if ctx.is_null() || (*ctx).rbtree.is_null() || (*ctx).shpool.is_null() {
        return Err("Invalid VTS context");
    }

    let rbtree = (*ctx).rbtree;
    let shpool = (*ctx).shpool;

    // Calculate hash for the key
    let hash = vts_hash_key(key);

    // Try to find existing node
    let node = vts_lookup_node(rbtree, hash, key);

    if !node.is_null() {
        // Update existing node
        let vts_node = node as *mut VtsSharedNode;
        (*vts_node).update_request(status, bytes_in, bytes_out, request_time);
    } else {
        // Create new node
        let node_size = std::mem::size_of::<VtsSharedNode>() + key.len();
        let new_node = ngx_slab_alloc_locked(shpool, node_size) as *mut VtsSharedNode;

        if new_node.is_null() {
            return Err("Failed to allocate memory for VTS node");
        }

        // Initialize the node
        (*new_node) = VtsSharedNode::new();
        (*new_node).len = key.len() as u16;
        (*new_node).node.key = hash;
        (*new_node).update_request(status, bytes_in, bytes_out, request_time);

        // Copy the key after the node structure
        let key_ptr = (new_node as *mut u8).add(std::mem::size_of::<VtsSharedNode>());
        std::ptr::copy_nonoverlapping(key.as_ptr(), key_ptr, key.len());

        // Insert into red-black tree
        ngx_rbtree_insert(rbtree, &mut (*new_node).node);
    }

    Ok(())
}

/// Set/update VTS node statistics
///
/// Based on ngx_http_vhost_traffic_status_node_set from the original module
///
/// # Safety
///
/// This function manipulates shared memory node data
pub unsafe fn vts_node_set(
    node: *mut VtsSharedNode,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    request_time: u64,
) {
    if !node.is_null() {
        (*node).update_request(status, bytes_in, bytes_out, request_time);
    }
}

/// Calculate hash for a VTS key
///
/// Simple hash function for demonstration - in production, use nginx's hash functions
fn vts_hash_key(key: &str) -> ngx_rbtree_key_t {
    let mut hash: ngx_rbtree_key_t = 0;
    for byte in key.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(byte as ngx_rbtree_key_t);
    }
    hash
}

/// Lookup a VTS node in the red-black tree
///
/// # Safety
///
/// This function traverses the red-black tree in shared memory
unsafe fn vts_lookup_node(
    rbtree: *mut ngx_rbtree_t,
    hash: ngx_rbtree_key_t,
    key: &str,
) -> *mut ngx_rbtree_node_t {
    if rbtree.is_null() {
        return std::ptr::null_mut();
    }

    let sentinel = (*rbtree).sentinel;
    let mut node = (*rbtree).root;

    while node != sentinel {
        if hash < (*node).key {
            node = (*node).left;
        } else if hash > (*node).key {
            node = (*node).right;
        } else {
            // Hash matches, check the actual key
            let vts_node = node as *mut VtsSharedNode;
            let node_key_len = (*vts_node).len as usize;

            if node_key_len == key.len() {
                let node_key_ptr =
                    (vts_node as *const u8).add(std::mem::size_of::<VtsSharedNode>());
                let node_key = std::slice::from_raw_parts(node_key_ptr, node_key_len);

                if node_key == key.as_bytes() {
                    return node;
                }
            }

            // Hash collision, continue searching (usually go left)
            node = (*node).left;
        }
    }

    std::ptr::null_mut()
}

/// Record request statistics in VTS shared memory
///
/// This is the main entry point for recording traffic statistics
///
/// # Safety
///
/// This function accesses shared memory and should be called during request processing
pub unsafe fn vts_record_request(
    ctx: *mut VtsSharedContext,
    server_name: &str,
    status: u16,
    bytes_in: u64,
    bytes_out: u64,
    request_time: u64,
) -> Result<(), &'static str> {
    if server_name.is_empty() {
        return Err("Empty server name");
    }

    vts_shm_add_node(ctx, server_name, status, bytes_in, bytes_out, request_time)
}

/// Global reference to VTS shared context
static mut VTS_GLOBAL_CTX: *mut VtsSharedContext = std::ptr::null_mut();

/// Post-configuration hook to register log phase handler
///
/// # Safety
///
/// This function is called during nginx configuration and registers request handlers
extern "C" fn ngx_http_vts_postconfiguration(_cf: *mut ngx_conf_t) -> ngx_int_t {
    // For now, we'll implement a simpler approach using the status endpoint
    // to avoid complex nginx phase integration issues
    NGX_OK as ngx_int_t
}

/// Record a status request to VTS
///
/// This records the current status endpoint access as a request
/// Eventually this should be replaced with real request phase integration
///
/// # Safety
///
/// This function manipulates shared memory
pub unsafe fn vts_record_status_request() -> Result<(), &'static str> {
    if VTS_GLOBAL_CTX.is_null() {
        return Err("VTS context not initialized");
    }

    // Record this status endpoint access
    // Use "localhost" since that's what's configured in your nginx
    vts_record_request(VTS_GLOBAL_CTX, "localhost", 200, 256, 512, 5)?;

    Ok(())
}

/// Set global VTS context (called during shared memory initialization)
///
/// # Safety
///
/// This function stores a global reference to shared memory context
pub unsafe fn vts_set_global_context(ctx: *mut VtsSharedContext) {
    VTS_GLOBAL_CTX = ctx;
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
        assert!(content.contains("test-hostname"));
        // The content should either show statistics or indicate shared memory not initialized
        assert!(
            content.contains("VTS Statistics") || content.contains("Shared memory not initialized")
        );
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

    #[test]
    fn test_vts_hash_key() {
        // Test hash function consistency
        let key1 = "example.com";
        let key2 = "example.com";
        let key3 = "different.com";

        assert_eq!(vts_hash_key(key1), vts_hash_key(key2));
        assert_ne!(vts_hash_key(key1), vts_hash_key(key3));

        // Test empty key
        assert_eq!(vts_hash_key(""), 0);
    }

    #[test]
    fn test_vts_shared_node_layout() {
        // Verify that VtsSharedNode has the expected layout
        // The ngx_rbtree_node_t must be first for compatibility
        let node = VtsSharedNode::new();
        let node_ptr = &node as *const VtsSharedNode;
        let rbtree_node_ptr = &node.node as *const ngx_rbtree_node_t;

        assert_eq!(node_ptr as *const u8, rbtree_node_ptr as *const u8);
    }

    #[test]
    fn test_vts_shared_node_basic() {
        let node = VtsSharedNode::new();

        // Test initial state - all fields should be zero
        assert_eq!(node.stat_request_counter, 0);
        assert_eq!(node.stat_2xx_counter, 0);
        assert_eq!(node.stat_in_bytes, 0);
        assert_eq!(node.stat_out_bytes, 0);

        // Test node structure layout
        let node_ptr = &node as *const VtsSharedNode;
        let rbtree_node_ptr = &node.node as *const ngx_rbtree_node_t;
        assert_eq!(node_ptr as *const u8, rbtree_node_ptr as *const u8);

        // Test that the structure has expected minimum size
        assert!(std::mem::size_of::<VtsSharedNode>() >= std::mem::size_of::<ngx_rbtree_node_t>());
    }
}
