// Integration test to demonstrate cache functionality
//
// This test manually adds cache data and verifies it appears in VTS output

#[test] 
fn test_cache_integration_demo() {
    let _lock = GLOBAL_VTS_TEST_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    // Clear all stats to start fresh
    CACHE_MANAGER.clear();
    {
        let mut manager = match VTS_MANAGER.write() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        *manager = VtsStatsManager::new();
    }

    // Simulate cache events that would occur during nginx request processing
    println!("=== Simulating Cache Events ===");
    
    // Simulate first request (cache MISS)
    update_cache_stats("cache_test", "MISS");
    update_cache_size("cache_test", 4194304, 512000); // 4MB max, 512KB used
    println!("First request: MISS - Cache now has 512KB/4MB used");

    // Simulate second request (cache HIT)
    update_cache_stats("cache_test", "HIT");
    update_cache_size("cache_test", 4194304, 512000); // Size unchanged
    println!("Second request: HIT - Cache size unchanged");

    // Simulate third request (cache HIT)
    update_cache_stats("cache_test", "HIT");
    println!("Third request: HIT");

    // Generate VTS status content with cache data
    let content = crate::prometheus::generate_vts_status_content();

    println!("=== VTS Output with Cache Statistics ===");
    
    // Extract cache section from output
    let cache_section_start = content.find("# HELP nginx_vts_cache_requests_total").unwrap_or(0);
    let cache_section_end = content.find("# HELP nginx_vts_server_requests_total")
        .unwrap_or(content.len());
    let cache_section = &content[cache_section_start..cache_section_end];
    
    println!("{}", cache_section);

    // Verify cache metrics are present and correct
    assert!(content.contains("nginx_vts_cache_requests_total{zone=\"cache_test\",status=\"hit\"} 2"));
    assert!(content.contains("nginx_vts_cache_requests_total{zone=\"cache_test\",status=\"miss\"} 1"));
    assert!(content.contains("nginx_vts_cache_size_bytes{zone=\"cache_test\",type=\"max\"} 4194304"));
    assert!(content.contains("nginx_vts_cache_size_bytes{zone=\"cache_test\",type=\"used\"} 512000"));
    assert!(content.contains("nginx_vts_cache_hit_ratio{zone=\"cache_test\"} 66.67"));

    println!("\n=== Cache Statistics Summary ===");
    let cache_zones = get_all_cache_zones();
    let cache_test_zone = cache_zones.get("cache_test").unwrap();
    println!("Zone: {}", cache_test_zone.name);
    println!("  Total Requests: {}", cache_test_zone.cache.total_requests());
    println!("  Cache Hits: {}", cache_test_zone.cache.hit);
    println!("  Cache Misses: {}", cache_test_zone.cache.miss);
    println!("  Hit Ratio: {:.2}%", cache_test_zone.cache.hit_ratio());
    println!("  Max Size: {} bytes ({:.1} MB)", 
        cache_test_zone.size.max_size, 
        cache_test_zone.size.max_size as f64 / 1024.0 / 1024.0);
    println!("  Used Size: {} bytes ({:.1} KB)", 
        cache_test_zone.size.used_size, 
        cache_test_zone.size.used_size as f64 / 1024.0);
    println!("  Utilization: {:.1}%", cache_test_zone.size.utilization_percentage());

    println!("\n✅ Cache functionality working correctly!");
    println!("   To integrate with nginx cache events, implement cache status hooks");
    println!("   in nginx configuration or module handlers to call update_cache_stats()");
}