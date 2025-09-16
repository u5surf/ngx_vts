// Test to verify ISSUE3.md resolution
// This test validates that nginx upstream configuration is recognized  
// and upstream zones are initialized properly showing in VTS status

mod issue3_test {
    use crate::{generate_vts_status_content, initialize_upstream_zones_for_testing, GLOBAL_VTS_TEST_MUTEX, VTS_MANAGER};
    
    #[test]
    fn test_issue3_upstream_zone_initialization() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        
        // Clear any existing data to simulate fresh nginx startup
        {
            let mut manager = match VTS_MANAGER.write() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        // Test 1: Initial state should have no upstream zones (like first curl)
        let initial_content = generate_vts_status_content();
        
        println!("=== Test 1: Initial State (No upstream zones) ===");
        println!("{}", initial_content);
        println!("=== End Test 1 ===");
        
        // Should show zero upstream zones initially
        assert!(initial_content.contains("nginx_vts_upstream_zones_total 0"));
        assert!(!initial_content.contains("# Upstream Zones:"));
        
        // Test 2: After initialization, upstream zones should be recognized 
        initialize_upstream_zones_for_testing();
        
        let after_init_content = generate_vts_status_content();
        
        println!("=== Test 2: After Upstream Zone Initialization ===");
        println!("{}", after_init_content);
        println!("=== End Test 2 ===");
        
        
        // Should show proper Prometheus metrics for the backend upstream
        assert!(after_init_content.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 0"));
        assert!(after_init_content.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 0"));
        assert!(after_init_content.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 0"));
        assert!(after_init_content.contains("nginx_vts_upstream_server_up{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        
        // Verify response time metrics are initialized to zero
        assert!(after_init_content.contains("nginx_vts_upstream_response_seconds{upstream=\"backend\",server=\"127.0.0.1:8080\",type=\"request_avg\"} 0.000000"));
        assert!(after_init_content.contains("nginx_vts_upstream_response_seconds{upstream=\"backend\",server=\"127.0.0.1:8080\",type=\"upstream_avg\"} 0.000000"));
        
        // Verify status code counters are all zero initially
        assert!(after_init_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"1xx\"} 0"));
        assert!(after_init_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 0"));
        assert!(after_init_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"3xx\"} 0"));
        assert!(after_init_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"4xx\"} 0"));
        assert!(after_init_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"5xx\"} 0"));
    }
    
    #[test]
    fn test_issue3_expected_response_format() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        
        // Clear and initialize
        {
            let mut manager = match VTS_MANAGER.write() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        initialize_upstream_zones_for_testing();
        
        let content = generate_vts_status_content();
        
        // Verify the response format matches ISSUE3.md expectation
        assert!(content.contains("# nginx-vts-rust"));
        assert!(content.contains("# VTS Status: Active"));
        assert!(content.contains("# Module: nginx-vts-rust"));
        
        
        // Should contain all Prometheus metrics from ISSUE3.md expected response
        assert!(content.contains("# HELP nginx_vts_upstream_requests_total Total upstream requests"));
        assert!(content.contains("# TYPE nginx_vts_upstream_requests_total counter"));
        assert!(content.contains("# HELP nginx_vts_upstream_bytes_total Total bytes transferred to/from upstream"));
        assert!(content.contains("# TYPE nginx_vts_upstream_bytes_total counter"));
        assert!(content.contains("# HELP nginx_vts_upstream_response_seconds Upstream response time statistics"));
        assert!(content.contains("# TYPE nginx_vts_upstream_response_seconds gauge"));
        assert!(content.contains("# HELP nginx_vts_upstream_server_up Upstream server status (1=up, 0=down)"));
        assert!(content.contains("# TYPE nginx_vts_upstream_server_up gauge"));
        assert!(content.contains("# HELP nginx_vts_upstream_responses_total Upstream responses by status code"));
        assert!(content.contains("# TYPE nginx_vts_upstream_responses_total counter"));
    }
    
    #[test]
    fn test_issue3_dynamic_request_tracking() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        
        // Clear and initialize
        {
            let mut manager = match VTS_MANAGER.write() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        initialize_upstream_zones_for_testing();
        
        // Verify initial state shows 0 requests (like first curl to /status)
        let initial_status = generate_vts_status_content();
        println!("=== Initial Status (After nginx startup) ===");
        println!("{}", initial_status);
        
        assert!(initial_status.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 0"));
        
        // Simulate the second curl request: curl -I http://localhost:8081/
        // This request goes through upstream backend to 127.0.0.1:8080
        use crate::update_upstream_zone_stats;
        
        update_upstream_zone_stats(
            "backend",
            "127.0.0.1:8080",
            94,   // request_time (ms) from ISSUE3.md example
            30,   // upstream_response_time (ms)
            1370, // bytes_sent
            615,  // bytes_received
            200   // status_code
        );
        
        // Verify third curl shows updated statistics (like third curl to /status)
        let after_request_status = generate_vts_status_content();
        println!("=== Status After One Request ===");
        println!("{}", after_request_status);
        
        // Should show the request was processed
        assert!(after_request_status.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        assert!(after_request_status.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 615"));
        assert!(after_request_status.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 1370"));
        assert!(after_request_status.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 1"));
        
        // Verify response time metrics are calculated
        assert!(after_request_status.contains("nginx_vts_upstream_response_seconds"));
        
        // Should show proper metrics instead of summary format
        assert!(after_request_status.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 1"));
    }
}