// Test to verify ISSUE2.md resolution
// This test validates that upstream statistics start from zero
// and update dynamically based on real requests

mod issue2_test {
    use crate::{generate_vts_status_content, update_upstream_zone_stats, vts_track_upstream_request, GLOBAL_VTS_TEST_MUTEX, VTS_MANAGER};
    use std::ffi::CString;
    
    #[test]
    fn test_issue2_zero_initialization() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap();
        
        // Clear all existing data to simulate fresh nginx startup
        if let Ok(mut manager) = VTS_MANAGER.write() {
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        // Generate initial VTS status content - should show no upstream zones
        let initial_content = generate_vts_status_content();
        
        println!("=== Initial Status (Fresh Startup) ===");
        println!("{}", initial_content);
        println!("=== End Initial Status ===");
        
        // Verify that initially no upstream zones exist
        assert!(!initial_content.contains("nginx_vts_upstream_requests_total"));
        
        // Should only show basic VTS info
        assert!(initial_content.contains("# nginx-vts-rust"));
        assert!(initial_content.contains("# VTS Status: Active"));
        assert!(initial_content.contains("# Prometheus Metrics:"));
        
        // The key test: should show empty metrics or only basic module info
        assert!(
            initial_content.contains("# HELP nginx_vts_info") || 
            initial_content.trim().ends_with("# Prometheus Metrics:")
        );
    }
    
    #[test]
    fn test_issue2_dynamic_request_tracking() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap();
        
        // Clear all existing data
        if let Ok(mut manager) = VTS_MANAGER.write() {
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        // Verify empty state
        let empty_content = generate_vts_status_content();
        assert!(!empty_content.contains("nginx_vts_upstream_requests_total"));
        
        // Simulate first request to http://localhost:8081/ -> upstream backend -> 127.0.0.1:8080
        update_upstream_zone_stats(
            "backend",
            "127.0.0.1:8080",
            85,  // request_time (ms)
            42,  // upstream_response_time (ms)
            1024, // bytes_sent
            512,  // bytes_received
            200,  // status_code
        );
        
        let after_first_request = generate_vts_status_content();
        
        println!("=== After First Request ===");
        println!("{}", after_first_request);
        println!("=== End After First Request ===");
        
        // Verify upstream statistics appeared with count = 1
        assert!(after_first_request.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        assert!(after_first_request.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 512"));
        assert!(after_first_request.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 1024"));
        assert!(after_first_request.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 1"));
        
        // Simulate second request 
        update_upstream_zone_stats(
            "backend",
            "127.0.0.1:8080",
            92,   // request_time (ms)
            48,   // upstream_response_time (ms)
            1536, // bytes_sent
            768,  // bytes_received
            200,  // status_code
        );
        
        let after_second_request = generate_vts_status_content();
        
        println!("=== After Second Request ===");
        println!("{}", after_second_request);
        println!("=== End After Second Request ===");
        
        // Verify statistics accumulated correctly
        assert!(after_second_request.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 2"));
        assert!(after_second_request.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 1280")); // 512 + 768
        assert!(after_second_request.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 2560")); // 1024 + 1536
        assert!(after_second_request.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 2"));
        
        // Verify response time calculations (average should be updated)
        assert!(after_second_request.contains("nginx_vts_upstream_response_seconds"));
    }
    
    #[test] 
    fn test_issue2_external_c_api() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap();
        
        // Clear state
        if let Ok(mut manager) = VTS_MANAGER.write() {
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        // Test the external C API
        let upstream_name = CString::new("backend").unwrap();
        let server_addr = CString::new("127.0.0.1:8080").unwrap();
        
        // Call the C API function
        unsafe {
            vts_track_upstream_request(
                upstream_name.as_ptr(),
                server_addr.as_ptr(),
                1000, // start_sec (simulated)
                500,  // start_msec (simulated)
                38,   // upstream_response_time  
                2048, // bytes_sent
                1024, // bytes_received
                200   // status_code
            );
        }
        
        let content = generate_vts_status_content();
        
        println!("=== After C API Call ===");
        println!("{}", content);
        println!("=== End After C API Call ===");
        
        // Verify the C API worked correctly
        assert!(content.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        assert!(content.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 1024"));
        assert!(content.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 2048"));
    }
}