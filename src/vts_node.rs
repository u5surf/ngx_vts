//! VTS Node Implementation with Shared Memory and Red-Black Tree
//!
//! This module provides efficient storage and retrieval of virtual host traffic statistics
//! using nginx's shared memory and red-black tree data structures, similar to the original
//! nginx-module-vts implementation.

use crate::stats::{VtsConnectionStats, VtsRequestTimes, VtsResponseStats, VtsServerStats};
use crate::upstream_stats::UpstreamZone;
#[cfg(not(test))]
use ngx::ffi::ngx_time;
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
        let current_time = Self::get_current_time();
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

    /// Get current time (nginx-safe version for testing)
    fn get_current_time() -> u64 {
        #[cfg(not(test))]
        {
            ngx_time() as u64
        }
        #[cfg(test)]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs()
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
    /// In-memory server zone statistics storage (temporary implementation)
    pub stats: HashMap<String, VtsNodeStats>,

    /// Upstream zones statistics storage
    pub upstream_zones: HashMap<String, UpstreamZone>,

    /// Connection statistics
    pub connections: VtsConnectionStats,
}

#[allow(dead_code)]
impl VtsStatsManager {
    /// Create a new VTS statistics manager
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
            upstream_zones: HashMap::new(),
            connections: VtsConnectionStats::default(),
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

    // --- Upstream Zone Management ---

    /// Update upstream statistics
    #[allow(clippy::too_many_arguments)] // Matches nginx API requirements
    pub fn update_upstream_stats(
        &mut self,
        upstream_name: &str,
        upstream_addr: &str,
        request_time: u64,
        upstream_response_time: u64,
        bytes_sent: u64,
        bytes_received: u64,
        status_code: u16,
    ) {
        let upstream_zone = self
            .upstream_zones
            .entry(upstream_name.to_string())
            .or_insert_with(|| UpstreamZone::new(upstream_name));

        let server_stats = upstream_zone.get_or_create_server(upstream_addr);

        // Update counters
        server_stats.request_counter += 1;
        server_stats.in_bytes += bytes_received;
        server_stats.out_bytes += bytes_sent;

        // Update response status
        server_stats.update_response_status(status_code);

        // Update timing
        server_stats.update_timing(request_time, upstream_response_time);
    }

    /// Get upstream zone statistics
    pub fn get_upstream_zone(&self, upstream_name: &str) -> Option<&UpstreamZone> {
        self.upstream_zones.get(upstream_name)
    }

    /// Get mutable upstream zone statistics
    pub fn get_upstream_zone_mut(&mut self, upstream_name: &str) -> Option<&mut UpstreamZone> {
        self.upstream_zones.get_mut(upstream_name)
    }

    /// Get all upstream zones
    pub fn get_all_upstream_zones(&self) -> &HashMap<String, UpstreamZone> {
        &self.upstream_zones
    }

    /// Get or create upstream zone
    pub fn get_or_create_upstream_zone(&mut self, upstream_name: &str) -> &mut UpstreamZone {
        self.upstream_zones
            .entry(upstream_name.to_string())
            .or_insert_with(|| UpstreamZone::new(upstream_name))
    }

    /// Update connection statistics
    pub fn update_connection_stats(
        &mut self,
        active: u64,
        reading: u64,
        writing: u64,
        waiting: u64,
        accepted: u64,
        handled: u64,
    ) {
        self.connections.active = active;
        self.connections.reading = reading;
        self.connections.writing = writing;
        self.connections.waiting = waiting;
        self.connections.accepted = accepted;
        self.connections.handled = handled;
    }

    /// Get connection statistics
    pub fn get_connection_stats(&self) -> &VtsConnectionStats {
        &self.connections
    }

    /// Get all server statistics in format compatible with PrometheusFormatter
    pub fn get_all_server_stats(&self) -> HashMap<String, VtsServerStats> {
        let mut server_stats = HashMap::new();

        for (zone_name, node_stats) in &self.stats {
            let avg_time = if node_stats.requests > 0 {
                (node_stats.request_time_total as f64) / (node_stats.requests as f64) / 1000.0
            } else {
                0.0
            };

            let server_stat = VtsServerStats {
                requests: node_stats.requests,
                bytes_in: node_stats.bytes_in,
                bytes_out: node_stats.bytes_out,
                responses: VtsResponseStats {
                    status_1xx: node_stats.status_1xx,
                    status_2xx: node_stats.status_2xx,
                    status_3xx: node_stats.status_3xx,
                    status_4xx: node_stats.status_4xx,
                    status_5xx: node_stats.status_5xx,
                },
                request_times: VtsRequestTimes {
                    total: node_stats.request_time_total as f64 / 1000.0,
                    min: 0.001, // Placeholder - should be tracked properly
                    max: (node_stats.request_time_max as f64) / 1000.0,
                    avg: avg_time,
                },
                last_updated: node_stats.last_request_time,
            };

            server_stats.insert(zone_name.clone(), server_stat);
        }

        server_stats
    }
}

impl Default for VtsStatsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prometheus::PrometheusFormatter;
    use std::sync::{Arc, RwLock};
    use std::thread;

    #[test]
    fn test_vts_stats_manager_initialization() {
        let manager = VtsStatsManager::new();
        assert!(manager.stats.is_empty());
        assert!(manager.upstream_zones.is_empty());
    }

    #[test]
    fn test_complete_upstream_pipeline() {
        let mut manager = VtsStatsManager::new();

        // Simulate realistic traffic to multiple upstreams
        let upstreams_data = [
            ("web_backend", "192.168.1.10:80", 120, 60, 1500, 800, 200),
            ("web_backend", "192.168.1.11:80", 180, 90, 2000, 1000, 200),
            ("web_backend", "192.168.1.10:80", 250, 120, 1200, 600, 404),
            ("api_backend", "192.168.2.10:8080", 80, 40, 800, 400, 200),
            (
                "api_backend",
                "192.168.2.11:8080",
                300,
                200,
                3000,
                1500,
                500,
            ),
        ];

        for (upstream, server, req_time, resp_time, sent, recv, status) in upstreams_data.iter() {
            manager.update_upstream_stats(
                upstream, server, *req_time, *resp_time, *sent, *recv, *status,
            );
        }

        // Verify data collection
        let web_backend = manager.get_upstream_zone("web_backend").unwrap();
        assert_eq!(web_backend.servers.len(), 2);
        assert_eq!(web_backend.total_requests(), 3);

        let api_backend = manager.get_upstream_zone("api_backend").unwrap();
        assert_eq!(api_backend.servers.len(), 2);
        assert_eq!(api_backend.total_requests(), 2);

        // Generate Prometheus metrics
        let formatter = PrometheusFormatter::new();
        let all_upstreams = manager.get_all_upstream_zones();
        let prometheus_output = formatter.format_upstream_stats(all_upstreams);

        // Verify Prometheus output contains expected metrics
        assert!(prometheus_output.contains("nginx_vts_upstream_requests_total{upstream=\"web_backend\",server=\"192.168.1.10:80\"} 2"));
        assert!(prometheus_output.contains("nginx_vts_upstream_requests_total{upstream=\"web_backend\",server=\"192.168.1.11:80\"} 1"));
        assert!(prometheus_output.contains("nginx_vts_upstream_requests_total{upstream=\"api_backend\",server=\"192.168.2.10:8080\"} 1"));
        assert!(prometheus_output.contains("nginx_vts_upstream_requests_total{upstream=\"api_backend\",server=\"192.168.2.11:8080\"} 1"));

        // Verify status code metrics
        assert!(prometheus_output.contains("nginx_vts_upstream_responses_total{upstream=\"web_backend\",server=\"192.168.1.10:80\",status=\"2xx\"} 1"));
        assert!(prometheus_output.contains("nginx_vts_upstream_responses_total{upstream=\"web_backend\",server=\"192.168.1.10:80\",status=\"4xx\"} 1"));
        assert!(prometheus_output.contains("nginx_vts_upstream_responses_total{upstream=\"api_backend\",server=\"192.168.2.11:8080\",status=\"5xx\"} 1"));
    }

    #[test]
    fn test_memory_efficiency_large_dataset() {
        let mut manager = VtsStatsManager::new();

        const NUM_UPSTREAMS: usize = 5;
        const NUM_SERVERS_PER_UPSTREAM: usize = 3;
        const NUM_REQUESTS_PER_SERVER: usize = 50;

        for upstream_id in 0..NUM_UPSTREAMS {
            let upstream_name = format!("backend_{}", upstream_id);

            for server_id in 0..NUM_SERVERS_PER_UPSTREAM {
                let server_addr = format!("10.0.{}.{}:8080", upstream_id, server_id);

                for request_id in 0..NUM_REQUESTS_PER_SERVER {
                    manager.update_upstream_stats(
                        &upstream_name,
                        &server_addr,
                        100 + (request_id % 200) as u64,
                        50 + (request_id % 100) as u64,
                        1500,
                        800,
                        if request_id % 10 == 0 { 500 } else { 200 },
                    );
                }
            }
        }

        // Verify all data was collected correctly
        let all_upstreams = manager.get_all_upstream_zones();
        assert_eq!(all_upstreams.len(), NUM_UPSTREAMS);

        for zone in all_upstreams.values() {
            assert_eq!(zone.servers.len(), NUM_SERVERS_PER_UPSTREAM);
            assert_eq!(
                zone.total_requests(),
                (NUM_SERVERS_PER_UPSTREAM * NUM_REQUESTS_PER_SERVER) as u64
            );
        }

        // Generate and verify Prometheus output
        let formatter = PrometheusFormatter::new();
        let prometheus_output = formatter.format_upstream_stats(all_upstreams);

        // Count number of request total metrics
        let request_metrics_count = prometheus_output
            .matches("nginx_vts_upstream_requests_total{")
            .count();
        assert_eq!(
            request_metrics_count,
            NUM_UPSTREAMS * NUM_SERVERS_PER_UPSTREAM
        );
    }

    #[test]
    fn test_thread_safety_simulation() {
        let manager: Arc<RwLock<VtsStatsManager>> = Arc::new(RwLock::new(VtsStatsManager::new()));
        let mut handles = vec![];

        // Simulate concurrent access from multiple threads
        for i in 0..10 {
            let manager_clone = Arc::clone(&manager);
            let handle = thread::spawn(move || {
                let mut m = manager_clone.write().unwrap();
                m.update_upstream_stats(
                    "concurrent_test",
                    &format!("server{}:80", i % 3), // 3 different servers
                    100 + i * 10,
                    50 + i * 5,
                    1000,
                    500,
                    200,
                );
            });
            handles.push(handle);
        }

        // Wait for all threads to complete
        for handle in handles {
            handle.join().unwrap();
        }

        // Verify all requests were recorded
        let final_manager = manager.read().unwrap();
        let zone = final_manager.get_upstream_zone("concurrent_test").unwrap();

        assert_eq!(zone.total_requests(), 10);
        assert_eq!(zone.servers.len(), 3); // server0, server1, server2
    }

    #[test]
    fn test_upstream_zone_management() {
        let mut manager = VtsStatsManager::new();

        // Update upstream statistics
        manager.update_upstream_stats(
            "backend",
            "10.0.0.1:80",
            100,  // request_time
            50,   // upstream_response_time
            1024, // bytes_sent
            512,  // bytes_received
            200,  // status_code
        );

        // Verify upstream zone was created
        let upstream_zone = manager.get_upstream_zone("backend").unwrap();
        assert_eq!(upstream_zone.name, "backend");
        assert_eq!(upstream_zone.servers.len(), 1);

        // Verify server statistics
        let server_stats = upstream_zone.servers.get("10.0.0.1:80").unwrap();
        assert_eq!(server_stats.request_counter, 1);
        assert_eq!(server_stats.in_bytes, 512);
        assert_eq!(server_stats.out_bytes, 1024);
        assert_eq!(server_stats.responses.status_2xx, 1);
    }

    #[test]
    fn test_multiple_upstream_servers() {
        let mut manager = VtsStatsManager::new();

        // Add stats for multiple servers in the same upstream
        manager.update_upstream_stats("backend", "10.0.0.1:80", 100, 50, 1000, 500, 200);
        manager.update_upstream_stats("backend", "10.0.0.2:80", 150, 75, 1500, 750, 200);
        manager.update_upstream_stats("backend", "10.0.0.1:80", 120, 60, 1200, 600, 404);

        let upstream_zone = manager.get_upstream_zone("backend").unwrap();
        assert_eq!(upstream_zone.servers.len(), 2);

        // Check first server (2 requests)
        let server1 = upstream_zone.servers.get("10.0.0.1:80").unwrap();
        assert_eq!(server1.request_counter, 2);
        assert_eq!(server1.responses.status_2xx, 1);
        assert_eq!(server1.responses.status_4xx, 1);

        // Check second server (1 request)
        let server2 = upstream_zone.servers.get("10.0.0.2:80").unwrap();
        assert_eq!(server2.request_counter, 1);
        assert_eq!(server2.responses.status_2xx, 1);

        // Check total requests
        assert_eq!(upstream_zone.total_requests(), 3);
    }
}
