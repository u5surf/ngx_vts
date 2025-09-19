//! Shared memory management for VTS module
//!
//! This module handles nginx shared memory initialization and red-black tree operations
//! for the VTS statistics storage.

use ngx::ffi::*;
use std::os::raw::c_void;

/// VTS shared memory context structure
///
/// Stores the red-black tree and slab pool for VTS statistics
#[repr(C)]
#[allow(dead_code)]
pub struct VtsSharedContext {
    /// Red-black tree for storing VTS nodes
    pub rbtree: *mut ngx_rbtree_t,
    /// Slab pool for memory allocation
    pub shpool: *mut ngx_slab_pool_t,
}

/// Custom red-black tree insert function for VTS nodes
///
/// # Safety
///
/// This function is called by nginx's red-black tree implementation
pub unsafe extern "C" fn vts_rbtree_insert_value(
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
pub extern "C" fn vts_init_shm_zone(shm_zone: *mut ngx_shm_zone_t, data: *mut c_void) -> ngx_int_t {
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
