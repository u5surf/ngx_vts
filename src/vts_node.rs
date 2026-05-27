//! Process-local fallback for VTS counters.
//!
//! Used when the `vts_zone` directive is not configured (or in unit
//! tests, where the slab allocator isn't available).  Storage uses
//! the same [`ServerCounters`] type as the shared-memory backend so
//! the conversion to the Prometheus-side [`VtsServerStats`] is
//! single-sourced.

use crate::shm::ServerCounters;
use crate::stats::{VtsConnectionStats, VtsServerStats};
use crate::upstream_stats::UpstreamZone;
use std::collections::HashMap;

/// Process-local VTS statistics manager.
///
/// Mirrors the public surface of the shared-memory backend so the
/// higher-level FFI can transparently fall through when no
/// `vts_zone` is configured (and so unit tests, which never link
/// the slab allocator, still have somewhere to write).
#[derive(Debug)]
#[allow(dead_code)]
pub struct VtsStatsManager {
    /// Per server-zone counters keyed by `server_name`.
    pub stats: HashMap<String, ServerCounters>,

    /// Per-upstream zone statistics.
    pub upstream_zones: HashMap<String, UpstreamZone>,

    /// Latest connection-state snapshot.
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
        self.stats
            .entry(server_name.to_string())
            .or_insert_with(ServerCounters::new)
            .update(status, bytes_in, bytes_out, request_time);
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
        self.stats
            .iter()
            .map(|(zone, counters)| (zone.clone(), (*counters).into_stats()))
            .collect()
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
    fn update_server_stats_records_through_server_counters() {
        // Reuses the same counter type as the shm backend, so we get
        // the proper min/max/avg request-time tracking for free
        // (previously vts_node had a placeholder `0` for min).
        let mut manager = VtsStatsManager::new();
        manager.update_server_stats("example.test", 200, 100, 1000, 50);
        manager.update_server_stats("example.test", 404, 50, 200, 200);
        manager.update_server_stats("example.test", 200, 75, 500, 120);

        let snap = manager.get_all_server_stats();
        let s = snap.get("example.test").expect("zone should exist");
        assert_eq!(s.requests, 3);
        assert_eq!(s.bytes_in, 225);
        assert_eq!(s.bytes_out, 1700);
        assert_eq!(s.responses.status_2xx, 2);
        assert_eq!(s.responses.status_4xx, 1);
        assert_eq!(s.request_times.min, 0.050);
        assert_eq!(s.request_times.max, 0.200);
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
