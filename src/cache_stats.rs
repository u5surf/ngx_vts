//! Cache statistics collection module for VTS
//!
//! This module provides data structures and functionality for collecting
//! and managing nginx proxy cache statistics including hit/miss ratios,
//! cache size information, and various cache status responses.

/// Cache zone statistics container
///
/// Contains comprehensive metrics about a specific cache zone including
/// size information, byte transfer statistics, and cache hit/miss data.
#[derive(Debug, Clone)]
pub struct CacheZoneStats {
    /// Name of the cache zone (from proxy_cache directive)
    pub name: String,
    
    /// Maximum cache size in bytes (from proxy_cache_path configuration)
    pub max_size: u64,
    
    /// Currently used cache size in bytes
    pub used_size: u64,
    
    /// Total bytes read from cache (cache hits)
    pub in_bytes: u64,
    
    /// Total bytes written to cache (cache misses and updates)
    pub out_bytes: u64,
    
    /// Detailed cache response statistics
    pub responses: CacheResponses,
}

/// Cache response status statistics
///
/// Tracks different types of cache responses based on the $upstream_cache_status
/// nginx variable. These correspond to various cache states and behaviors.
#[derive(Debug, Clone, Default)]
pub struct CacheResponses {
    /// Cache miss - request was not found in cache
    pub miss: u64,
    
    /// Cache bypass - request bypassed cache due to configuration
    pub bypass: u64,
    
    /// Cache expired - cached content was expired and revalidated
    pub expired: u64,
    
    /// Cache stale - served stale content while updating
    pub stale: u64,
    
    /// Cache updating - response is being updated in background
    pub updating: u64,
    
    /// Cache revalidated - cached content was successfully revalidated
    pub revalidated: u64,
    
    /// Cache hit - request was successfully served from cache
    pub hit: u64,
    
    /// Cache scarce - could not cache due to insufficient memory
    pub scarce: u64,
}

/// Cache status enumeration
///
/// Represents the different possible values of the $upstream_cache_status variable
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CacheStatus {
    /// Request was not found in cache
    Miss,
    /// Request bypassed cache due to configuration
    Bypass,
    /// Cached content was expired
    Expired,
    /// Served stale content while updating
    Stale,
    /// Response is being updated in background
    Updating,
    /// Cached content was successfully revalidated
    Revalidated,
    /// Request was successfully served from cache
    Hit,
    /// Could not cache due to insufficient memory
    Scarce,
}

impl CacheZoneStats {
    /// Create new cache zone statistics
    ///
    /// # Arguments
    ///
    /// * `name` - Name of the cache zone
    /// * `max_size` - Maximum cache size in bytes (0 if unlimited)
    ///
    /// # Returns
    ///
    /// New CacheZoneStats instance with zero counters
    pub fn new(name: &str, max_size: u64) -> Self {
        Self {
            name: name.to_string(),
            max_size,
            used_size: 0,
            in_bytes: 0,
            out_bytes: 0,
            responses: CacheResponses::default(),
        }
    }
    
    /// Update cache statistics based on cache status
    ///
    /// # Arguments
    ///
    /// * `status` - Cache status from $upstream_cache_status
    /// * `bytes_transferred` - Number of bytes transferred for this request
    pub fn update_cache_access(&mut self, status: CacheStatus, bytes_transferred: u64) {
        match status {
            CacheStatus::Hit => {
                self.responses.hit += 1;
                self.in_bytes += bytes_transferred; // Read from cache
            }
            CacheStatus::Miss => {
                self.responses.miss += 1;
                self.out_bytes += bytes_transferred; // Write to cache
            }
            CacheStatus::Expired => {
                self.responses.expired += 1;
                self.out_bytes += bytes_transferred; // Refresh cache
            }
            CacheStatus::Bypass => {
                self.responses.bypass += 1;
                // No cache I/O for bypass
            }
            CacheStatus::Stale => {
                self.responses.stale += 1;
                self.in_bytes += bytes_transferred; // Read stale from cache
            }
            CacheStatus::Updating => {
                self.responses.updating += 1;
                self.in_bytes += bytes_transferred; // Read while updating
            }
            CacheStatus::Revalidated => {
                self.responses.revalidated += 1;
                self.in_bytes += bytes_transferred; // Read revalidated content
            }
            CacheStatus::Scarce => {
                self.responses.scarce += 1;
                // No cache I/O due to memory constraints
            }
        }
    }
    
    /// Update the current cache size
    ///
    /// # Arguments
    ///
    /// * `used_size` - Current cache size in bytes
    pub fn update_cache_size(&mut self, used_size: u64) {
        self.used_size = used_size;
    }
    
    /// Calculate cache hit ratio
    ///
    /// # Returns
    ///
    /// Hit ratio as a percentage (0.0 to 100.0), or 0.0 if no requests
    pub fn hit_ratio(&self) -> f64 {
        let total_requests = self.total_requests();
        if total_requests > 0 {
            (self.responses.hit as f64 / total_requests as f64) * 100.0
        } else {
            0.0
        }
    }
    
    /// Calculate cache utilization percentage
    ///
    /// # Returns
    ///
    /// Cache utilization as a percentage (0.0 to 100.0), or 0.0 if unlimited
    pub fn utilization(&self) -> f64 {
        if self.max_size > 0 {
            (self.used_size as f64 / self.max_size as f64) * 100.0
        } else {
            0.0 // Unlimited cache
        }
    }
    
    /// Get total number of cache requests
    ///
    /// # Returns
    ///
    /// Sum of all cache response counters
    pub fn total_requests(&self) -> u64 {
        self.responses.miss
            + self.responses.bypass
            + self.responses.expired
            + self.responses.stale
            + self.responses.updating
            + self.responses.revalidated
            + self.responses.hit
            + self.responses.scarce
    }
    
    /// Get total bytes transferred (in + out)
    ///
    /// # Returns
    ///
    /// Total bytes transferred through this cache zone
    pub fn total_bytes(&self) -> u64 {
        self.in_bytes + self.out_bytes
    }
}

impl CacheStatus {
    /// Parse cache status from string
    ///
    /// # Arguments
    ///
    /// * `status_str` - Status string from $upstream_cache_status variable
    ///
    /// # Returns
    ///
    /// Parsed CacheStatus or None if invalid
    pub fn from_str(status_str: &str) -> Option<Self> {
        match status_str.to_uppercase().as_str() {
            "HIT" => Some(CacheStatus::Hit),
            "MISS" => Some(CacheStatus::Miss),
            "EXPIRED" => Some(CacheStatus::Expired),
            "BYPASS" => Some(CacheStatus::Bypass),
            "STALE" => Some(CacheStatus::Stale),
            "UPDATING" => Some(CacheStatus::Updating),
            "REVALIDATED" => Some(CacheStatus::Revalidated),
            "SCARCE" => Some(CacheStatus::Scarce),
            _ => None,
        }
    }
    
    /// Convert cache status to string
    ///
    /// # Returns
    ///
    /// String representation of the cache status
    pub fn to_string(&self) -> &'static str {
        match self {
            CacheStatus::Hit => "hit",
            CacheStatus::Miss => "miss",
            CacheStatus::Expired => "expired",
            CacheStatus::Bypass => "bypass",
            CacheStatus::Stale => "stale",
            CacheStatus::Updating => "updating",
            CacheStatus::Revalidated => "revalidated",
            CacheStatus::Scarce => "scarce",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_cache_zone_stats_new() {
        let stats = CacheZoneStats::new("my_cache", 1073741824); // 1GB
        assert_eq!(stats.name, "my_cache");
        assert_eq!(stats.max_size, 1073741824);
        assert_eq!(stats.used_size, 0);
        assert_eq!(stats.in_bytes, 0);
        assert_eq!(stats.out_bytes, 0);
        assert_eq!(stats.total_requests(), 0);
    }
    
    #[test]
    fn test_cache_status_from_str() {
        assert_eq!(CacheStatus::from_str("HIT"), Some(CacheStatus::Hit));
        assert_eq!(CacheStatus::from_str("hit"), Some(CacheStatus::Hit));
        assert_eq!(CacheStatus::from_str("MISS"), Some(CacheStatus::Miss));
        assert_eq!(CacheStatus::from_str("EXPIRED"), Some(CacheStatus::Expired));
        assert_eq!(CacheStatus::from_str("invalid"), None);
    }
    
    #[test]
    fn test_cache_status_to_string() {
        assert_eq!(CacheStatus::Hit.to_string(), "hit");
        assert_eq!(CacheStatus::Miss.to_string(), "miss");
        assert_eq!(CacheStatus::Expired.to_string(), "expired");
    }
    
    #[test]
    fn test_update_cache_access() {
        let mut stats = CacheZoneStats::new("test_cache", 1024 * 1024);
        
        // Test cache hit
        stats.update_cache_access(CacheStatus::Hit, 500);
        assert_eq!(stats.responses.hit, 1);
        assert_eq!(stats.in_bytes, 500);
        assert_eq!(stats.out_bytes, 0);
        
        // Test cache miss
        stats.update_cache_access(CacheStatus::Miss, 300);
        assert_eq!(stats.responses.miss, 1);
        assert_eq!(stats.in_bytes, 500);
        assert_eq!(stats.out_bytes, 300);
        
        // Test bypass (no I/O)
        stats.update_cache_access(CacheStatus::Bypass, 200);
        assert_eq!(stats.responses.bypass, 1);
        assert_eq!(stats.in_bytes, 500);
        assert_eq!(stats.out_bytes, 300);
        
        assert_eq!(stats.total_requests(), 3);
    }
    
    #[test]
    fn test_hit_ratio() {
        let mut stats = CacheZoneStats::new("test_cache", 1024);
        
        // No requests yet
        assert_eq!(stats.hit_ratio(), 0.0);
        
        // Add some hits and misses
        stats.responses.hit = 8;
        stats.responses.miss = 2;
        
        assert_eq!(stats.hit_ratio(), 80.0);
    }
    
    #[test]
    fn test_utilization() {
        let mut stats = CacheZoneStats::new("test_cache", 1000);
        
        // Empty cache
        assert_eq!(stats.utilization(), 0.0);
        
        // Half full
        stats.update_cache_size(500);
        assert_eq!(stats.utilization(), 50.0);
        
        // Unlimited cache (max_size = 0)
        stats.max_size = 0;
        assert_eq!(stats.utilization(), 0.0);
    }
    
    #[test]
    fn test_total_bytes() {
        let mut stats = CacheZoneStats::new("test_cache", 1024);
        stats.in_bytes = 1000;
        stats.out_bytes = 500;
        
        assert_eq!(stats.total_bytes(), 1500);
    }
}