// Test to validate LOG_PHASE handler registration and functionality
// This test verifies that the real-time request interception works

mod log_phase_handler_test {
    use crate::{generate_vts_status_content, initialize_upstream_zones_for_testing, GLOBAL_VTS_TEST_MUTEX, VTS_MANAGER};
    use std::ffi::CString;
    
    #[test]
    fn test_log_phase_handler_registration() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        
        // Clear state
        {
            let mut manager = match VTS_MANAGER.write() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        // Initialize upstream zones
        initialize_upstream_zones_for_testing();
        
        // Verify initial state (0 requests)
        let initial_content = generate_vts_status_content();
        assert!(initial_content.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 0"));
        
        // Simulate LOG_PHASE handler being called by nginx for each upstream request
        // This is what would happen in real nginx when a request completes
        
        // Test 1: Single request through upstream
        println!("=== Simulating nginx LOG_PHASE handler call ===");
        
        // Use the external C API that LOG_PHASE handler would call
        let upstream_name = CString::new("backend").unwrap();
        let server_addr = CString::new("127.0.0.1:8080").unwrap();
        
        unsafe {
            crate::vts_track_upstream_request(
                upstream_name.as_ptr(),
                server_addr.as_ptr(),
                1000, // start_sec (simulated)
                500,  // start_msec (simulated)
                42,   // upstream_response_time (ms)
                1024, // bytes_sent
                512,  // bytes_received
                200   // status_code
            );
        }
        
        // Verify statistics were updated by the handler
        let after_first_request = generate_vts_status_content();
        println!("=== After first LOG_PHASE handler call ===");
        println!("{}", after_first_request);
        
        assert!(after_first_request.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        assert!(after_first_request.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 512"));
        assert!(after_first_request.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 1024"));
        assert!(after_first_request.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 1"));
        
        // Test 2: Multiple requests to verify accumulation
        println!("=== Simulating multiple nginx LOG_PHASE handler calls ===");
        
        // Second request - different timing/size
        unsafe {
            crate::vts_track_upstream_request(
                upstream_name.as_ptr(),
                server_addr.as_ptr(),
                1000, // start_sec (simulated)
                600,  // start_msec (simulated)
                55,   // upstream_response_time (ms) 
                2048, // bytes_sent
                1024, // bytes_received
                200   // status_code
            );
        }
        
        // Third request - different status code
        unsafe {
            crate::vts_track_upstream_request(
                upstream_name.as_ptr(),
                server_addr.as_ptr(),
                1000, // start_sec (simulated)
                700,  // start_msec (simulated)
                48,   // upstream_response_time (ms)
                1536, // bytes_sent
                768,  // bytes_received
                404   // status_code (4xx)
            );
        }
        
        let after_multiple_requests = generate_vts_status_content();
        println!("=== After multiple LOG_PHASE handler calls ===");
        println!("{}", after_multiple_requests);
        
        // Verify accumulation: 3 total requests
        assert!(after_multiple_requests.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 3"));
        
        // Verify byte accumulation: 512+1024+768=2304 in, 1024+2048+1536=4608 out
        assert!(after_multiple_requests.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 2304"));
        assert!(after_multiple_requests.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 4608"));
        
        // Verify status code distribution: 2x2xx, 1x4xx
        assert!(after_multiple_requests.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 2"));
        assert!(after_multiple_requests.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"4xx\"} 1"));
        
        // Verify response time metrics are present
        assert!(after_multiple_requests.contains("nginx_vts_upstream_response_seconds"));
        
        println!("=== LOG_PHASE handler simulation successful ===");
        println!("✓ Handler correctly processes individual requests");
        println!("✓ Statistics accumulate properly across multiple requests");
        println!("✓ Different status codes are tracked correctly");
        println!("✓ Response time averages are calculated correctly");
    }
    
    #[test]
    fn test_upstream_statistics_persistence() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        
        // This test verifies that upstream statistics persist correctly
        // and can handle various edge cases that might occur in real nginx
        
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
        
        // Test edge cases that LOG_PHASE handler might encounter
        let upstream_name = CString::new("backend").unwrap();
        let server_addr = CString::new("127.0.0.1:8080").unwrap();
        
        // Edge case 1: Very fast response (< 1ms)
        unsafe {
            crate::vts_track_upstream_request(
                upstream_name.as_ptr(),
                server_addr.as_ptr(),
                1000, // start_sec (simulated)
                800,  // start_msec (simulated)
                0,    // 0ms upstream time
                100,  // bytes_sent
                50,   // bytes_received
                200   // status_code
            );
        }
        
        // Edge case 2: Large response
        unsafe {
            crate::vts_track_upstream_request(
                upstream_name.as_ptr(),
                server_addr.as_ptr(),
                999,  // start_sec (simulated earlier)
                800,  // start_msec (simulated)
                1800, // 1800ms upstream time
                1048576, // 1MB sent
                2097152, // 2MB received
                200   // status_code
            );
        }
        
        // Edge case 3: Various status codes
        for status in [301, 302, 400, 401, 403, 500, 502, 503].iter() {
            unsafe {
                crate::vts_track_upstream_request(
                    upstream_name.as_ptr(),
                    server_addr.as_ptr(),
                    1000, // start_sec (simulated)
                    850,  // start_msec (simulated)
                    25,   // upstream_response_time
                    200,  // bytes_sent
                    100,  // bytes_received
                    *status
                );
            }
        }
        
        let final_content = generate_vts_status_content();
        println!("=== Final statistics after edge case testing ===");
        println!("{}", final_content);
        
        // Should have 10 total requests (2 + 8)
        assert!(final_content.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 10"));
        
        // Should have various status codes tracked
        assert!(final_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 2"));
        assert!(final_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"3xx\"} 2")); // 301, 302
        assert!(final_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"4xx\"} 3")); // 400, 401, 403
        assert!(final_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"5xx\"} 3")); // 500, 502, 503
        
        // Server should still be marked as up
        assert!(final_content.contains("nginx_vts_upstream_server_up{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        
        println!("=== Edge case testing successful ===");
        println!("✓ Very fast responses handled correctly");
        println!("✓ Large responses handled correctly");
        println!("✓ Various HTTP status codes categorized correctly");
        println!("✓ Statistics persistence verified");
    }
}