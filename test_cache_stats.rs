// Cache statistics tests
//
// This module contains comprehensive tests for cache statistics functionality

#[test]
fn test_cache_stats_basic_functionality() {
    let _lock = GLOBAL_VTS_TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Clear any existing cache stats
    CACHE_MANAGER.clear();

    // Add some cache statistics
    update_cache_stats("zone1", "HIT");
    update_cache_stats("zone1", "HIT");
    update_cache_stats("zone1", "MISS");
    update_cache_stats("zone1", "BYPASS");

    update_cache_size("zone1", 1048576, 524288); // 1MB max, 512KB used

    // Get cache statistics
    let cache_zones = get_all_cache_zones();
    assert_eq!(cache_zones.len(), 1);
    
    let zone1 = cache_zones.get("zone1").unwrap();
    assert_eq!(zone1.name, "zone1");
    assert_eq!(zone1.cache.hit, 2);
    assert_eq!(zone1.cache.miss, 1);
    assert_eq!(zone1.cache.bypass, 1);
    assert_eq!(zone1.cache.total_requests(), 4);
    assert_eq!(zone1.cache.hit_ratio(), 50.0);
    assert_eq!(zone1.size.max_size, 1048576);
    assert_eq!(zone1.size.used_size, 524288);
    assert_eq!(zone1.size.utilization_percentage(), 50.0);
}

#[test]
fn test_cache_stats_multiple_zones() {
    let _lock = GLOBAL_VTS_TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Clear any existing cache stats
    CACHE_MANAGER.clear();

    // Add statistics for multiple zones
    update_cache_stats("zone1", "HIT");
    update_cache_stats("zone1", "MISS");
    update_cache_stats("zone2", "HIT");
    update_cache_stats("zone2", "HIT");
    update_cache_stats("zone2", "HIT");

    update_cache_size("zone1", 1048576, 262144); // 1MB max, 256KB used
    update_cache_size("zone2", 2097152, 1572864); // 2MB max, 1.5MB used

    let cache_zones = get_all_cache_zones();
    assert_eq!(cache_zones.len(), 2);

    let zone1 = cache_zones.get("zone1").unwrap();
    assert_eq!(zone1.cache.hit, 1);
    assert_eq!(zone1.cache.miss, 1);
    assert_eq!(zone1.cache.hit_ratio(), 50.0);

    let zone2 = cache_zones.get("zone2").unwrap();
    assert_eq!(zone2.cache.hit, 3);
    assert_eq!(zone2.cache.miss, 0);
    assert_eq!(zone2.cache.hit_ratio(), 100.0);
    assert_eq!(zone2.size.utilization_percentage(), 75.0);
}

#[test]
fn test_cache_stats_all_statuses() {
    let _lock = GLOBAL_VTS_TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Clear any existing cache stats
    CACHE_MANAGER.clear();

    // Test all cache status types
    update_cache_stats("comprehensive_zone", "HIT");
    update_cache_stats("comprehensive_zone", "MISS");
    update_cache_stats("comprehensive_zone", "BYPASS");
    update_cache_stats("comprehensive_zone", "EXPIRED");
    update_cache_stats("comprehensive_zone", "STALE");
    update_cache_stats("comprehensive_zone", "UPDATING");
    update_cache_stats("comprehensive_zone", "REVALIDATED");
    update_cache_stats("comprehensive_zone", "SCARCE");

    let cache_zones = get_all_cache_zones();
    let zone = cache_zones.get("comprehensive_zone").unwrap();
    
    assert_eq!(zone.cache.hit, 1);
    assert_eq!(zone.cache.miss, 1);
    assert_eq!(zone.cache.bypass, 1);
    assert_eq!(zone.cache.expired, 1);
    assert_eq!(zone.cache.stale, 1);
    assert_eq!(zone.cache.updating, 1);
    assert_eq!(zone.cache.revalidated, 1);
    assert_eq!(zone.cache.scarce, 1);
    assert_eq!(zone.cache.total_requests(), 8);
    assert_eq!(zone.cache.hit_ratio(), 12.5); // 1/8 = 12.5%
}

#[test]
fn test_cache_metrics_in_status_output() {
    let _lock = GLOBAL_VTS_TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Clear all stats
    CACHE_MANAGER.clear();
    {
        let mut manager = match VTS_MANAGER.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *manager = VtsStatsManager::new();
    }

    // Add cache statistics
    update_cache_stats("test_cache", "HIT");
    update_cache_stats("test_cache", "HIT");
    update_cache_stats("test_cache", "MISS");
    update_cache_size("test_cache", 1048576, 524288);

    // Generate VTS status content
    let content = crate::prometheus::generate_vts_status_content();

    // Verify cache metrics are present
    assert!(content.contains("# HELP nginx_vts_cache_requests_total"));
    assert!(content.contains("# TYPE nginx_vts_cache_requests_total counter"));
    assert!(content.contains("nginx_vts_cache_requests_total{zone=\"test_cache\",status=\"hit\"} 2"));
    assert!(content.contains("nginx_vts_cache_requests_total{zone=\"test_cache\",status=\"miss\"} 1"));
    
    assert!(content.contains("# HELP nginx_vts_cache_size_bytes"));
    assert!(content.contains("# TYPE nginx_vts_cache_size_bytes gauge"));
    assert!(content.contains("nginx_vts_cache_size_bytes{zone=\"test_cache\",type=\"max\"} 1048576"));
    assert!(content.contains("nginx_vts_cache_size_bytes{zone=\"test_cache\",type=\"used\"} 524288"));
    
    assert!(content.contains("# HELP nginx_vts_cache_hit_ratio"));
    assert!(content.contains("# TYPE nginx_vts_cache_hit_ratio gauge"));
    assert!(content.contains("nginx_vts_cache_hit_ratio{zone=\"test_cache\"} 66.67"));
}

#[test]
fn test_empty_cache_metrics() {
    let _lock = GLOBAL_VTS_TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Clear all stats to ensure empty state
    CACHE_MANAGER.clear();
    {
        let mut manager = match VTS_MANAGER.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *manager = VtsStatsManager::new();
    }

    // Generate VTS status content with no cache data
    let content = crate::prometheus::generate_vts_status_content();

    // Should still have headers even with no data
    assert!(content.contains("# HELP nginx_vts_cache_requests_total"));
    assert!(content.contains("# TYPE nginx_vts_cache_requests_total counter"));
    assert!(content.contains("# HELP nginx_vts_cache_size_bytes"));
    assert!(content.contains("# TYPE nginx_vts_cache_size_bytes gauge"));
}