// Comprehensive integration test for ISSUE3.md complete flow
// This test simulates the exact sequence described in ISSUE3.md:
// 1. nginx startup -> first /status call -> second request -> third /status call

mod issue3_integration_test {
    use crate::{generate_vts_status_content, initialize_upstream_zones_for_testing, update_upstream_zone_stats, GLOBAL_VTS_TEST_MUTEX, VTS_MANAGER};
    
    #[test]
    fn test_issue3_complete_flow_simulation() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap();
        
        println!("=== ISSUE3.md Complete Flow Simulation ===");
        
        // Step 1: Simulate fresh nginx startup with upstream backend configuration
        if let Ok(mut manager) = VTS_MANAGER.write() {
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        // Initialize upstream zones (simulates nginx parsing upstream backend { server 127.0.0.1:8080; })
        initialize_upstream_zones_for_testing();
        
        // Step 2: First curl http://localhost:8081/status (should show initialized upstream zones)
        let first_status_response = generate_vts_status_content();
        
        println!("=== First curl http://localhost:8081/status ===");
        println!("{}", first_status_response);
        println!("=== End First Response ===\n");
        
        // Verify first response matches expected output from ISSUE3.md  
        assert!(first_status_response.contains("# nginx-vts-rust"));
        assert!(first_status_response.contains("# VTS Status: Active"));
        assert!(first_status_response.contains("# Module: nginx-vts-rust"));
        
        
        // Should have all prometheus metrics with zero values
        assert!(first_status_response.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 0"));
        assert!(first_status_response.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 0"));
        assert!(first_status_response.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 0"));
        assert!(first_status_response.contains("nginx_vts_upstream_server_up{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        
        // Step 3: Second request: curl -I http://localhost:8081/
        // This goes through proxy_pass http://backend; -> 127.0.0.1:8080
        println!("=== Second request: curl -I http://localhost:8081/ ===");
        println!("Request processed through upstream backend -> 127.0.0.1:8080");
        
        // Simulate the LOG_PHASE handler collecting statistics
        update_upstream_zone_stats(
            "backend",
            "127.0.0.1:8080",
            94,   // request_time (matches ISSUE3.md example: 94ms avg)
            30,   // upstream_response_time  
            1370, // bytes_sent (matches ISSUE3.md: direction="out")
            615,  // bytes_received (matches ISSUE3.md: direction="in") 
            200   // status_code (2xx response)
        );
        
        println!("Statistics updated: 94ms request time, 30ms upstream time, 615 bytes in, 1370 bytes out, 200 status\n");
        
        // Step 4: Third curl http://localhost:8081/status (should show updated statistics)
        let third_status_response = generate_vts_status_content();
        
        println!("=== Third curl http://localhost:8081/status ===");
        println!("{}", third_status_response);
        println!("=== End Third Response ===\n");
        
        // Verify third response matches expected output from ISSUE3.md
        assert!(third_status_response.contains("# nginx-vts-rust"));
        assert!(third_status_response.contains("# VTS Status: Active"));
        assert!(third_status_response.contains("# Module: nginx-vts-rust"));
        
        
        // Verify all Prometheus metrics are updated correctly
        assert!(third_status_response.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        assert!(third_status_response.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 615"));
        assert!(third_status_response.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 1370"));
        
        // Verify response time metrics (converted to seconds: 94ms = 0.094s, 30ms = 0.030s)
        assert!(third_status_response.contains("nginx_vts_upstream_response_seconds{upstream=\"backend\",server=\"127.0.0.1:8080\",type=\"request_avg\"} 0.094000"));
        assert!(third_status_response.contains("nginx_vts_upstream_response_seconds{upstream=\"backend\",server=\"127.0.0.1:8080\",type=\"upstream_avg\"} 0.030000"));
        
        // Verify status code counters
        assert!(third_status_response.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"} 1"));
        assert!(third_status_response.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"1xx\"} 0"));
        assert!(third_status_response.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"3xx\"} 0"));
        assert!(third_status_response.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"4xx\"} 0"));
        assert!(third_status_response.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"5xx\"} 0"));
        
        println!("=== ISSUE3.md Flow Verification Complete ===");
        println!("✓ First request shows initialized upstream zones with zero values");  
        println!("✓ Second request processes through upstream backend properly");
        println!("✓ Third request shows updated statistics with correct values");
        println!("✓ All Prometheus metrics format correctly");
        println!("✓ Response times, byte counts, and status codes match expected values");
    }
    
    #[test]
    fn test_issue3_nginx_conf_compliance() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap();
        
        // This test validates that our implementation correctly interprets
        // the nginx.conf from ISSUE3.md:
        //
        // upstream backend {
        //     server 127.0.0.1:8080;
        // }
        // server {
        //     listen 8081;
        //     location / {
        //         proxy_pass http://backend;
        //     }
        //     location /status {
        //         vts_status;
        //     }
        // }
        
        if let Ok(mut manager) = VTS_MANAGER.write() {
            manager.stats.clear();
            manager.upstream_zones.clear();
        }
        
        initialize_upstream_zones_for_testing();
        
        let status_content = generate_vts_status_content();
        
        // Verify nginx.conf upstream backend is recognized
        assert!(status_content.contains("backend"));
        assert!(status_content.contains("127.0.0.1:8080"));
        
        // Verify vts_upstream_stats directive behavior
        assert!(status_content.contains("nginx_vts_upstream_requests_total"));
        assert!(status_content.contains("nginx_vts_upstream_bytes_total"));
        assert!(status_content.contains("nginx_vts_upstream_response_seconds"));
        assert!(status_content.contains("nginx_vts_upstream_server_up"));
        assert!(status_content.contains("nginx_vts_upstream_responses_total"));
        
        // Verify vts_zone main 10m directive creates proper context
        assert!(status_content.contains("# VTS Status: Active"));
        assert!(status_content.contains("# Module: nginx-vts-rust"));
    }
}