//! Upstream statistics collection module for VTS
//!
//! This module provides data structures and functionality for collecting
//! and managing upstream server statistics including request counts,
//! byte transfers, response times, and server status information.

use std::collections::HashMap;

/// Response statistics structure (reused from stats.rs design)
#[derive(Debug, Clone, Default)]
pub struct VtsResponseStats {
    /// 1xx status responses
    pub status_1xx: u64,
    /// 2xx status responses  
    pub status_2xx: u64,
    /// 3xx status responses
    pub status_3xx: u64,
    /// 4xx status responses
    pub status_4xx: u64,
    /// 5xx status responses
    pub status_5xx: u64,
}

/// Statistics for an individual upstream server
///
/// Contains comprehensive metrics about a specific upstream server including
/// request/response data, timing information, and nginx configuration status.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some fields are for future nginx integration
pub struct UpstreamServerStats {
    /// Server address in format "host:port" (e.g., "10.10.10.11:80")
    pub server: String,

    /// Total number of requests sent to this server
    pub request_counter: u64,

    /// Total bytes received from this server
    pub in_bytes: u64,

    /// Total bytes sent to this server
    pub out_bytes: u64,

    /// Response status code statistics (reusing existing structure)
    pub responses: VtsResponseStats,

    /// Total request processing time in milliseconds
    pub request_time_total: u64,

    /// Counter for request time measurements (for average calculation)
    pub request_time_counter: u64,

    /// Total upstream response time in milliseconds
    pub response_time_total: u64,

    /// Counter for response time measurements (for average calculation)
    pub response_time_counter: u64,

    /// Server weight from nginx configuration
    pub weight: u32,

    /// Max fails setting from nginx configuration
    pub max_fails: u32,

    /// Fail timeout setting in seconds from nginx configuration
    pub fail_timeout: u32,

    /// Whether this server is marked as backup
    pub backup: bool,

    /// Whether this server is currently marked as down
    pub down: bool,
}

/// Statistics container for an upstream group
///
/// Contains all server statistics for a named upstream group,
/// allowing tracking of multiple servers within the same upstream block.
#[derive(Debug, Clone)]
#[allow(dead_code)] // Some fields are for future nginx integration
pub struct UpstreamZone {
    /// Name of the upstream group (from nginx configuration)
    pub name: String,

    /// Map of server address to its statistics
    /// Key: server address (e.g., "10.10.10.11:80")
    /// Value: statistics for that server
    pub servers: HashMap<String, UpstreamServerStats>,
}

impl UpstreamServerStats {
    /// Create new upstream server statistics with default values
    ///
    /// # Arguments
    ///
    /// * `server` - Server address string (e.g., "10.10.10.11:80")
    ///
    /// # Returns
    ///
    /// New UpstreamServerStats instance with zero counters
    pub fn new(server: &str) -> Self {
        Self {
            server: server.to_string(),
            request_counter: 0,
            in_bytes: 0,
            out_bytes: 0,
            responses: VtsResponseStats::default(),
            request_time_total: 0,
            request_time_counter: 0,
            response_time_total: 0,
            response_time_counter: 0,
            weight: 1,
            max_fails: 1,
            fail_timeout: 10,
            backup: false,
            down: false,
        }
    }

    /// Update response status statistics
    ///
    /// # Arguments
    ///
    /// * `status_code` - HTTP status code from upstream response
    pub fn update_response_status(&mut self, status_code: u16) {
        match status_code {
            100..=199 => self.responses.status_1xx += 1,
            200..=299 => self.responses.status_2xx += 1,
            300..=399 => self.responses.status_3xx += 1,
            400..=499 => self.responses.status_4xx += 1,
            500..=599 => self.responses.status_5xx += 1,
            _ => {}
        }
    }

    /// Update timing statistics
    ///
    /// # Arguments
    ///
    /// * `request_time` - Total request processing time in milliseconds
    /// * `upstream_response_time` - Upstream response time in milliseconds
    pub fn update_timing(&mut self, request_time: u64, upstream_response_time: u64) {
        if request_time > 0 {
            self.request_time_total += request_time;
            self.request_time_counter += 1;
        }

        if upstream_response_time > 0 {
            self.response_time_total += upstream_response_time;
            self.response_time_counter += 1;
        }
    }

    /// Get average request processing time
    ///
    /// # Returns
    ///
    /// Average request time in milliseconds, or 0.0 if no requests recorded
    #[allow(dead_code)] // Used in prometheus formatter
    pub fn avg_request_time(&self) -> f64 {
        if self.request_time_counter > 0 {
            self.request_time_total as f64 / self.request_time_counter as f64
        } else {
            0.0
        }
    }

    /// Get average upstream response time
    ///
    /// # Returns
    ///
    /// Average response time in milliseconds, or 0.0 if no responses recorded
    #[allow(dead_code)] // Used in prometheus formatter
    pub fn avg_response_time(&self) -> f64 {
        if self.response_time_counter > 0 {
            self.response_time_total as f64 / self.response_time_counter as f64
        } else {
            0.0
        }
    }
}

impl UpstreamZone {
    /// Create new upstream zone
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the upstream group
    ///
    /// # Returns
    ///
    /// New UpstreamZone instance with empty servers map
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            servers: HashMap::new(),
        }
    }

    /// Get or create server statistics entry
    ///
    /// # Arguments
    ///
    /// * `server_addr` - Server address string
    ///
    /// # Returns
    ///
    /// Mutable reference to server statistics
    pub fn get_or_create_server(&mut self, server_addr: &str) -> &mut UpstreamServerStats {
        self.servers
            .entry(server_addr.to_string())
            .or_insert_with(|| UpstreamServerStats::new(server_addr))
    }

    /// Get total request count for all servers in this upstream
    ///
    /// # Returns
    ///
    /// Sum of request counters from all servers
    #[allow(dead_code)] // Used in tests and future integrations
    pub fn total_requests(&self) -> u64 {
        self.servers.values().map(|s| s.request_counter).sum()
    }

    /// Get total bytes transferred (in + out) for all servers
    ///
    /// # Returns
    ///
    /// Tuple of (total_in_bytes, total_out_bytes)
    #[allow(dead_code)] // Used in tests and future integrations
    pub fn total_bytes(&self) -> (u64, u64) {
        let total_in = self.servers.values().map(|s| s.in_bytes).sum();
        let total_out = self.servers.values().map(|s| s.out_bytes).sum();
        (total_in, total_out)
    }
}

#[cfg(test)]
#[allow(clippy::items_after_test_module)] // Large refactor needed to move, allow for now
mod tests {
    use super::*;

    #[test]
    fn test_upstream_server_stats_new() {
        let stats = UpstreamServerStats::new("192.168.1.1:80");
        assert_eq!(stats.server, "192.168.1.1:80");
        assert_eq!(stats.request_counter, 0);
        assert_eq!(stats.in_bytes, 0);
        assert_eq!(stats.out_bytes, 0);
        assert_eq!(stats.weight, 1);
        assert!(!stats.backup);
        assert!(!stats.down);
    }

    #[test]
    fn test_update_response_status() {
        let mut stats = UpstreamServerStats::new("test:80");

        stats.update_response_status(200);
        stats.update_response_status(404);
        stats.update_response_status(500);

        assert_eq!(stats.responses.status_2xx, 1);
        assert_eq!(stats.responses.status_4xx, 1);
        assert_eq!(stats.responses.status_5xx, 1);
    }

    #[test]
    fn test_update_timing() {
        let mut stats = UpstreamServerStats::new("test:80");

        stats.update_timing(100, 50);
        stats.update_timing(200, 75);

        assert_eq!(stats.request_time_total, 300);
        assert_eq!(stats.request_time_counter, 2);
        assert_eq!(stats.response_time_total, 125);
        assert_eq!(stats.response_time_counter, 2);

        assert_eq!(stats.avg_request_time(), 150.0);
        assert_eq!(stats.avg_response_time(), 62.5);
    }

    #[test]
    fn test_upstream_zone() {
        let mut zone = UpstreamZone::new("backend");
        assert_eq!(zone.name, "backend");
        assert!(zone.servers.is_empty());

        let server1 = zone.get_or_create_server("10.0.0.1:80");
        server1.request_counter = 100;
        server1.in_bytes = 1000;
        server1.out_bytes = 500;

        let server2 = zone.get_or_create_server("10.0.0.2:80");
        server2.request_counter = 200;
        server2.in_bytes = 2000;
        server2.out_bytes = 1000;

        assert_eq!(zone.total_requests(), 300);
        assert_eq!(zone.total_bytes(), (3000, 1500));
    }

}


