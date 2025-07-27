//! VTS Node Implementation with Shared Memory and Red-Black Tree
//!
//! This module provides efficient storage and retrieval of virtual host traffic statistics
//! using nginx's shared memory and red-black tree data structures, similar to the original
//! nginx-module-vts implementation.

use ngx::ffi::*;
use std::collections::HashMap;

/// VTS Node statistics data structure
///
/// Stores traffic statistics for a specific virtual host or server zone
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VtsNodeStats {
    /// Total number of requests
    pub requests: u64,

    /// Total bytes received from clients
    pub bytes_in: u64,

    /// Total bytes sent to clients  
    pub bytes_out: u64,

    /// Response status code counters
    pub status_1xx: u64,
    pub status_2xx: u64,
    pub status_3xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,

    /// Request timing statistics
    pub request_time_total: u64, // Total request time in milliseconds
    pub request_time_max: u64, // Maximum request time in milliseconds

    /// Timestamp of first request
    pub first_request_time: u64,

    /// Timestamp of last request
    pub last_request_time: u64,
}

#[allow(dead_code)]
impl VtsNodeStats {
    /// Create a new VTS node with zero statistics
    pub fn new() -> Self {
        Self {
            requests: 0,
            bytes_in: 0,
            bytes_out: 0,
            status_1xx: 0,
            status_2xx: 0,
            status_3xx: 0,
            status_4xx: 0,
            status_5xx: 0,
            request_time_total: 0,
            request_time_max: 0,
            first_request_time: 0,
            last_request_time: 0,
        }
    }

    /// Update statistics with a new request
    pub fn update_request(
        &mut self,
        status: u16,
        bytes_in: u64,
        bytes_out: u64,
        request_time: u64,
    ) {
        self.requests += 1;
        self.bytes_in += bytes_in;
        self.bytes_out += bytes_out;
        self.request_time_total += request_time;

        // Update max request time
        if request_time > self.request_time_max {
            self.request_time_max = request_time;
        }

        // Update status counters
        match status {
            100..=199 => self.status_1xx += 1,
            200..=299 => self.status_2xx += 1,
            300..=399 => self.status_3xx += 1,
            400..=499 => self.status_4xx += 1,
            500..=599 => self.status_5xx += 1,
            _ => {} // Ignore invalid status codes
        }

        // Update timestamps
        let current_time = ngx_time() as u64;
        if self.first_request_time == 0 {
            self.first_request_time = current_time;
        }
        self.last_request_time = current_time;
    }

    /// Get average request time in milliseconds
    pub fn avg_request_time(&self) -> f64 {
        if self.requests > 0 {
            self.request_time_total as f64 / self.requests as f64
        } else {
            0.0
        }
    }
}

impl Default for VtsNodeStats {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple VTS statistics manager (without shared memory for now)
///
/// This will be replaced with shared memory implementation later
#[derive(Debug)]
#[allow(dead_code)]
pub struct VtsStatsManager {
    /// In-memory statistics storage (temporary implementation)
    pub stats: HashMap<String, VtsNodeStats>,
}

#[allow(dead_code)]
impl VtsStatsManager {
    /// Create a new VTS statistics manager
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
        }
    }

    /// Update statistics for a server zone
    pub fn update_server_stats(
        &mut self,
        server_name: &str,
        status: u16,
        bytes_in: u64,
        bytes_out: u64,
        request_time: u64,
    ) {
        let stats = self.stats.entry(server_name.to_string()).or_default();
        stats.update_request(status, bytes_in, bytes_out, request_time);
    }

    /// Get statistics for a server zone
    pub fn get_server_stats(&self, server_name: &str) -> Option<&VtsNodeStats> {
        self.stats.get(server_name)
    }

    /// Get all server statistics
    pub fn get_all_stats(&self) -> Vec<(String, VtsNodeStats)> {
        self.stats
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }
}

impl Default for VtsStatsManager {
    fn default() -> Self {
        Self::new()
    }
}

/// VTS Node structure for shared memory storage
///
/// This structure is stored directly in nginx shared memory and matches
/// the layout of the original nginx-module-vts node structure
#[repr(C)]
#[allow(dead_code)]
pub struct VtsSharedNode {
    /// Red-black tree node (must be first for nginx compatibility)
    pub node: ngx_rbtree_node_t,

    /// Node length (for variable-length data)
    pub len: u16,

    /// Node type flags
    pub stat_upstream: u8,

    /// Reserved padding
    pub reserved: u8,

    /// Request statistics
    pub stat_request_counter: u64,
    pub stat_in_bytes: u64,
    pub stat_out_bytes: u64,

    /// Response status counters
    pub stat_1xx_counter: u64,
    pub stat_2xx_counter: u64,
    pub stat_3xx_counter: u64,
    pub stat_4xx_counter: u64,
    pub stat_5xx_counter: u64,

    /// Request timing
    pub stat_request_time_counter: u64,
    pub stat_request_time: u64,

    /// Cache statistics
    pub stat_cache_max_size: u64,
    pub stat_cache_used_size: u64,

    /// Time tracking
    pub stat_request_time_start: ngx_msec_t,
    pub stat_request_time_end: ngx_msec_t,
}

impl VtsSharedNode {
    /// Create a new VTS shared node with zero statistics
    pub fn new() -> Self {
        unsafe { std::mem::zeroed() }
    }

    /// Update node with request statistics
    pub unsafe fn update_request(
        &mut self,
        status: u16,
        bytes_in: u64,
        bytes_out: u64,
        request_time: u64,
    ) {
        self.stat_request_counter += 1;
        self.stat_in_bytes += bytes_in;
        self.stat_out_bytes += bytes_out;
        self.stat_request_time_counter += request_time;

        // Update status counters
        match status {
            100..=199 => self.stat_1xx_counter += 1,
            200..=299 => self.stat_2xx_counter += 1,
            300..=399 => self.stat_3xx_counter += 1,
            400..=499 => self.stat_4xx_counter += 1,
            500..=599 => self.stat_5xx_counter += 1,
            _ => {} // Ignore invalid status codes
        }

        // Update timing
        let current_time = ngx_time() as ngx_msec_t;
        if self.stat_request_time_start == 0 {
            self.stat_request_time_start = current_time;
        }
        self.stat_request_time_end = current_time;

        // Calculate average request time
        if self.stat_request_counter > 0 {
            self.stat_request_time = self.stat_request_time_counter / self.stat_request_counter;
        }
    }
}

impl Default for VtsSharedNode {
    fn default() -> Self {
        Self::new()
    }
}
