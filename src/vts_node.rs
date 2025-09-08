//! VTS Node Implementation with Shared Memory and Red-Black Tree
//!
//! This module provides efficient storage and retrieval of virtual host traffic statistics
//! using nginx's shared memory and red-black tree data structures, similar to the original
//! nginx-module-vts implementation.

use ngx::ffi::*;
use std::collections::HashMap;
use crate::upstream_stats::UpstreamZone;
use crate::cache_stats::{CacheZoneStats, CacheStatus};

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
    /// In-memory server zone statistics storage (temporary implementation)
    pub stats: HashMap<String, VtsNodeStats>,
    
    /// Upstream zones statistics storage
    pub upstream_zones: HashMap<String, UpstreamZone>,
    
    /// Cache zones statistics storage  
    pub cache_zones: HashMap<String, CacheZoneStats>,
}

#[allow(dead_code)]
impl VtsStatsManager {
    /// Create a new VTS statistics manager
    pub fn new() -> Self {
        Self {
            stats: HashMap::new(),
            upstream_zones: HashMap::new(),
            cache_zones: HashMap::new(),
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
        let upstream_zone = self.upstream_zones
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

    // --- Cache Zone Management ---

    /// Update cache statistics
    pub fn update_cache_stats(
        &mut self,
        cache_zone_name: &str,
        cache_status: CacheStatus,
        bytes_transferred: u64,
    ) {
        let cache_zone = self.cache_zones
            .entry(cache_zone_name.to_string())
            .or_insert_with(|| CacheZoneStats::new(cache_zone_name, 0)); // 0 means unlimited size

        cache_zone.update_cache_access(cache_status, bytes_transferred);
    }

    /// Update cache zone size
    pub fn update_cache_size(&mut self, cache_zone_name: &str, used_size: u64, max_size: Option<u64>) {
        let cache_zone = self.cache_zones
            .entry(cache_zone_name.to_string())
            .or_insert_with(|| CacheZoneStats::new(cache_zone_name, max_size.unwrap_or(0)));

        if let Some(max) = max_size {
            cache_zone.max_size = max;
        }
        cache_zone.update_cache_size(used_size);
    }

    /// Get cache zone statistics
    pub fn get_cache_zone(&self, cache_zone_name: &str) -> Option<&CacheZoneStats> {
        self.cache_zones.get(cache_zone_name)
    }

    /// Get mutable cache zone statistics
    pub fn get_cache_zone_mut(&mut self, cache_zone_name: &str) -> Option<&mut CacheZoneStats> {
        self.cache_zones.get_mut(cache_zone_name)
    }

    /// Get all cache zones
    pub fn get_all_cache_zones(&self) -> &HashMap<String, CacheZoneStats> {
        &self.cache_zones
    }

    /// Get or create cache zone
    pub fn get_or_create_cache_zone(&mut self, cache_zone_name: &str, max_size: u64) -> &mut CacheZoneStats {
        self.cache_zones
            .entry(cache_zone_name.to_string())
            .or_insert_with(|| CacheZoneStats::new(cache_zone_name, max_size))
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
    
    #[test]
    fn test_vts_stats_manager_initialization() {
        let manager = VtsStatsManager::new();
        assert!(manager.stats.is_empty());
        assert!(manager.upstream_zones.is_empty());
        assert!(manager.cache_zones.is_empty());
    }
    
    #[test]
    fn test_upstream_zone_management() {
        let mut manager = VtsStatsManager::new();
        
        // Update upstream statistics
        manager.update_upstream_stats(
            "backend",
            "10.0.0.1:80",
            100, // request_time
            50,  // upstream_response_time
            1024, // bytes_sent
            512,  // bytes_received
            200   // status_code
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
    fn test_cache_zone_management() {
        let mut manager = VtsStatsManager::new();
        
        // Update cache statistics
        manager.update_cache_stats(
            "my_cache",
            CacheStatus::Hit,
            2048
        );
        
        // Verify cache zone was created
        let cache_zone = manager.get_cache_zone("my_cache").unwrap();
        assert_eq!(cache_zone.name, "my_cache");
        assert_eq!(cache_zone.responses.hit, 1);
        assert_eq!(cache_zone.in_bytes, 2048);
        assert_eq!(cache_zone.out_bytes, 0);
        
        // Update cache size
        manager.update_cache_size("my_cache", 1048576, Some(10485760)); // 1MB used, 10MB max
        
        let cache_zone = manager.get_cache_zone("my_cache").unwrap();
        assert_eq!(cache_zone.used_size, 1048576);
        assert_eq!(cache_zone.max_size, 10485760);
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
    
    #[test]
    fn test_cache_hit_ratio() {
        let mut manager = VtsStatsManager::new();
        
        // Add cache hits and misses
        manager.update_cache_stats("test_cache", CacheStatus::Hit, 1000);
        manager.update_cache_stats("test_cache", CacheStatus::Hit, 1000);
        manager.update_cache_stats("test_cache", CacheStatus::Hit, 1000);
        manager.update_cache_stats("test_cache", CacheStatus::Miss, 500);
        manager.update_cache_stats("test_cache", CacheStatus::Miss, 500);
        
        let cache_zone = manager.get_cache_zone("test_cache").unwrap();
        assert_eq!(cache_zone.responses.hit, 3);
        assert_eq!(cache_zone.responses.miss, 2);
        assert_eq!(cache_zone.hit_ratio(), 60.0); // 3 hits out of 5 total = 60%
    }
}
