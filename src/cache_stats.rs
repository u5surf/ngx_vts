//! Cache statistics collection module for VTS
//!
//! This module provides data structures and functionality for collecting
//! and managing cache statistics including hit/miss ratios, cache sizes,
//! and cache status information for both server zones and upstream servers.

use std::collections::HashMap;
use std::sync::RwLock;

/// Cache status statistics
///
/// Tracks cache hit/miss statistics following nginx-module-vts implementation
#[derive(Debug, Clone, Default)]
pub struct VtsCacheStats {
    /// Cache miss count (requests that resulted in upstream fetch)
    pub miss: u64,
    /// Cache bypass count (requests that bypassed cache)
    pub bypass: u64,
    /// Cache expired count (cached content that expired and was refetched)
    pub expired: u64,
    /// Cache stale count (stale content served from cache)
    pub stale: u64,
    /// Cache updating count (cache being updated in background)
    pub updating: u64,
    /// Cache revalidated count (content revalidated with upstream)
    pub revalidated: u64,
    /// Cache hit count (successful cache hits)
    pub hit: u64,
    /// Cache scarce count (cache storage low, content evicted)
    pub scarce: u64,
}

/// Cache size statistics
///
/// Tracks cache memory usage information
#[derive(Debug, Clone, Default)]
pub struct VtsCacheSizeStats {
    /// Maximum cache size in bytes
    pub max_size: u64,
    /// Currently used cache size in bytes
    pub used_size: u64,
}

/// Combined cache statistics for a cache zone
///
/// Combines both status and size statistics for comprehensive cache monitoring
#[derive(Debug, Clone, Default)]
pub struct CacheZoneStats {
    /// Cache zone name
    pub name: String,
    /// Cache hit/miss statistics
    pub cache: VtsCacheStats,
    /// Cache size statistics
    pub size: VtsCacheSizeStats,
}

impl VtsCacheStats {
    /// Create new cache statistics with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Update cache statistics based on cache status
    ///
    /// # Arguments
    ///
    /// * `cache_status` - Cache status string (e.g., "HIT", "MISS", "BYPASS")
    pub fn update_cache_status(&mut self, cache_status: &str) {
        match cache_status.to_uppercase().as_str() {
            "HIT" => self.hit += 1,
            "MISS" => self.miss += 1,
            "BYPASS" => self.bypass += 1,
            "EXPIRED" => self.expired += 1,
            "STALE" => self.stale += 1,
            "UPDATING" => self.updating += 1,
            "REVALIDATED" => self.revalidated += 1,
            "SCARCE" => self.scarce += 1,
            _ => {} // Unknown cache status, ignore
        }
    }

    /// Get total cache requests (all cache operations)
    pub fn total_requests(&self) -> u64 {
        self.hit
            + self.miss
            + self.bypass
            + self.expired
            + self.stale
            + self.updating
            + self.revalidated
            + self.scarce
    }

    /// Get cache hit ratio as percentage
    ///
    /// # Returns
    ///
    /// Cache hit ratio as f64 (0.0 to 100.0), or 0.0 if no requests
    pub fn hit_ratio(&self) -> f64 {
        let total = self.total_requests();
        if total == 0 {
            0.0
        } else {
            (self.hit as f64 / total as f64) * 100.0
        }
    }
}

impl VtsCacheSizeStats {
    /// Create new cache size statistics
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum cache size in bytes
    /// * `used_size` - Currently used cache size in bytes
    pub fn new(max_size: u64, used_size: u64) -> Self {
        Self {
            max_size,
            used_size,
        }
    }

    /// Update cache size statistics
    ///
    /// # Arguments
    ///
    /// * `used_size` - Currently used cache size in bytes
    pub fn update_used_size(&mut self, used_size: u64) {
        self.used_size = used_size;
    }

    /// Get cache utilization percentage
    ///
    /// # Returns
    ///
    /// Cache utilization as f64 (0.0 to 100.0), or 0.0 if max_size is 0
    pub fn utilization_percentage(&self) -> f64 {
        if self.max_size == 0 {
            0.0
        } else {
            (self.used_size as f64 / self.max_size as f64) * 100.0
        }
    }
}

impl CacheZoneStats {
    /// Create new cache zone statistics
    ///
    /// # Arguments
    ///
    /// * `name` - Cache zone name
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            cache: VtsCacheStats::default(),
            size: VtsCacheSizeStats::default(),
        }
    }

    /// Update cache status for this zone
    ///
    /// # Arguments
    ///
    /// * `cache_status` - Cache status string (e.g., "HIT", "MISS", "BYPASS")
    pub fn update_cache_status(&mut self, cache_status: &str) {
        self.cache.update_cache_status(cache_status);
    }

    /// Update cache size information
    ///
    /// # Arguments
    ///
    /// * `max_size` - Maximum cache size in bytes
    /// * `used_size` - Currently used cache size in bytes
    pub fn update_cache_size(&mut self, max_size: u64, used_size: u64) {
        self.size.max_size = max_size;
        self.size.used_size = used_size;
    }
}

/// Cache statistics manager
///
/// Manages cache statistics for multiple cache zones
pub struct CacheStatsManager {
    /// Map of cache zone name to its statistics
    cache_zones: RwLock<HashMap<String, CacheZoneStats>>,
}

impl CacheStatsManager {
    /// Create new cache statistics manager
    pub fn new() -> Self {
        Self {
            cache_zones: RwLock::new(HashMap::new()),
        }
    }

    /// Update cache statistics for a specific zone
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Cache zone name
    /// * `cache_status` - Cache status string (e.g., "HIT", "MISS", "BYPASS")
    pub fn update_cache_stats(&self, zone_name: &str, cache_status: &str) {
        let mut zones = self
            .cache_zones
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let zone_stats = zones
            .entry(zone_name.to_string())
            .or_insert_with(|| CacheZoneStats::new(zone_name));
        zone_stats.update_cache_status(cache_status);
    }

    /// Update cache size information for a specific zone
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Cache zone name  
    /// * `max_size` - Maximum cache size in bytes
    /// * `used_size` - Currently used cache size in bytes
    pub fn update_cache_size(&self, zone_name: &str, max_size: u64, used_size: u64) {
        let mut zones = self
            .cache_zones
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let zone_stats = zones
            .entry(zone_name.to_string())
            .or_insert_with(|| CacheZoneStats::new(zone_name));
        zone_stats.update_cache_size(max_size, used_size);
    }

    /// Get cache statistics for a specific zone
    ///
    /// # Arguments
    ///
    /// * `zone_name` - Cache zone name
    ///
    /// # Returns
    ///
    /// Option containing CacheZoneStats if zone exists
    #[allow(dead_code)] // Used in tests
    pub fn get_cache_zone(&self, zone_name: &str) -> Option<CacheZoneStats> {
        let zones = self
            .cache_zones
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        zones.get(zone_name).cloned()
    }

    /// Get all cache zone statistics
    ///
    /// # Returns
    ///
    /// HashMap containing all cache zone statistics
    pub fn get_all_cache_zones(&self) -> HashMap<String, CacheZoneStats> {
        let zones = self
            .cache_zones
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        zones.clone()
    }

    /// Clear all cache statistics
    #[allow(dead_code)] // Used in tests
    pub fn clear(&self) {
        let mut zones = self
            .cache_zones
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        zones.clear();
    }
}

impl Default for CacheStatsManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_stats_new() {
        let stats = VtsCacheStats::new();
        assert_eq!(stats.hit, 0);
        assert_eq!(stats.miss, 0);
        assert_eq!(stats.total_requests(), 0);
        assert_eq!(stats.hit_ratio(), 0.0);
    }

    #[test]
    fn test_cache_stats_update() {
        let mut stats = VtsCacheStats::new();

        stats.update_cache_status("HIT");
        stats.update_cache_status("HIT");
        stats.update_cache_status("MISS");
        stats.update_cache_status("BYPASS");

        assert_eq!(stats.hit, 2);
        assert_eq!(stats.miss, 1);
        assert_eq!(stats.bypass, 1);
        assert_eq!(stats.total_requests(), 4);
        assert_eq!(stats.hit_ratio(), 50.0);
    }

    #[test]
    fn test_cache_stats_unknown_status() {
        let mut stats = VtsCacheStats::new();
        stats.update_cache_status("UNKNOWN");
        assert_eq!(stats.total_requests(), 0);
    }

    #[test]
    fn test_cache_size_stats() {
        let mut size_stats = VtsCacheSizeStats::new(1000, 500);
        assert_eq!(size_stats.max_size, 1000);
        assert_eq!(size_stats.used_size, 500);
        assert_eq!(size_stats.utilization_percentage(), 50.0);

        size_stats.update_used_size(750);
        assert_eq!(size_stats.used_size, 750);
        assert_eq!(size_stats.utilization_percentage(), 75.0);
    }

    #[test]
    fn test_cache_zone_stats() {
        let mut zone = CacheZoneStats::new("test_zone");
        assert_eq!(zone.name, "test_zone");

        zone.update_cache_status("HIT");
        zone.update_cache_size(2048, 1024);

        assert_eq!(zone.cache.hit, 1);
        assert_eq!(zone.size.max_size, 2048);
        assert_eq!(zone.size.used_size, 1024);
        assert_eq!(zone.size.utilization_percentage(), 50.0);
    }

    #[test]
    fn test_cache_stats_manager() {
        let manager = CacheStatsManager::new();

        manager.update_cache_stats("zone1", "HIT");
        manager.update_cache_stats("zone1", "MISS");
        manager.update_cache_size("zone1", 1000, 500);

        let zone_stats = manager.get_cache_zone("zone1").unwrap();
        assert_eq!(zone_stats.name, "zone1");
        assert_eq!(zone_stats.cache.hit, 1);
        assert_eq!(zone_stats.cache.miss, 1);
        assert_eq!(zone_stats.size.max_size, 1000);
        assert_eq!(zone_stats.size.used_size, 500);

        let all_zones = manager.get_all_cache_zones();
        assert_eq!(all_zones.len(), 1);
        assert!(all_zones.contains_key("zone1"));
    }

    #[test]
    fn test_cache_stats_multiple_zones() {
        let manager = CacheStatsManager::new();

        manager.update_cache_stats("zone1", "HIT");
        manager.update_cache_stats("zone2", "MISS");

        let all_zones = manager.get_all_cache_zones();
        assert_eq!(all_zones.len(), 2);

        let zone1 = all_zones.get("zone1").unwrap();
        let zone2 = all_zones.get("zone2").unwrap();

        assert_eq!(zone1.cache.hit, 1);
        assert_eq!(zone2.cache.miss, 1);
    }

    #[test]
    fn test_cache_stats_clear() {
        let manager = CacheStatsManager::new();

        manager.update_cache_stats("zone1", "HIT");
        manager.clear();

        let all_zones = manager.get_all_cache_zones();
        assert_eq!(all_zones.len(), 0);
    }
}
