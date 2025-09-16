// Test to verify ISSUE1.md resolution
// This test specifically validates that the backend upstream with 127.0.0.1:8080
// server shows statistics as expected in the issue.

mod issue1_test {
    use crate::{generate_vts_status_content, GLOBAL_VTS_TEST_MUTEX, VTS_MANAGER};
    
    #[test]
    fn test_issue1_backend_upstream_statistics() {
        let _lock = GLOBAL_VTS_TEST_MUTEX.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        
        // Simulate the specific scenario from ISSUE1.md:
        // - upstream backend { server 127.0.0.1:8080; }
        // - vts_upstream_stats on;
        
        // Initialize upstream statistics for the exact backend mentioned in ISSUE1.md
        {
            let mut manager = match VTS_MANAGER.write() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            // Clear any existing data
            manager.upstream_zones.clear();
            
            // Add statistics for the backend upstream with 127.0.0.1:8080 server
            // Simulate multiple requests like in a real scenario
            for i in 0..500 {
                let status_code = if i % 50 == 0 { 500 } else if i % 20 == 0 { 404 } else { 200 };
                let response_time = 40 + (i % 30); // Vary response times
                let upstream_time = response_time / 2;
                
                manager.update_upstream_stats(
                    "backend",
                    "127.0.0.1:8080",
                    response_time,
                    upstream_time,
                    1500,  // bytes_sent
                    750,   // bytes_received  
                    status_code,
                );
            }
        }
        
        // Generate VTS status content
        let status_content = generate_vts_status_content();
        
        println!("=== ISSUE1.md Resolution Test Output ===");
        println!("{}", status_content);
        println!("=== End ISSUE1.md Test Output ===");
        
        // Verify the content contains the expected backend upstream statistics
        assert!(status_content.contains("nginx_vts_upstream_requests_total{upstream=\"backend\",server=\"127.0.0.1:8080\"} 500"));
        assert!(status_content.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"in\"} 375000"));
        assert!(status_content.contains("nginx_vts_upstream_bytes_total{upstream=\"backend\",server=\"127.0.0.1:8080\",direction=\"out\"} 750000"));
        
        // Verify response time metrics exist
        assert!(status_content.contains("nginx_vts_upstream_response_seconds{upstream=\"backend\",server=\"127.0.0.1:8080\",type=\"request_avg\"}"));
        assert!(status_content.contains("nginx_vts_upstream_response_seconds{upstream=\"backend\",server=\"127.0.0.1:8080\",type=\"upstream_avg\"}"));
        
        // Verify status code metrics
        assert!(status_content.contains("nginx_vts_upstream_responses_total{upstream=\"backend\",server=\"127.0.0.1:8080\",status=\"2xx\"}"));
        assert!(status_content.contains("nginx_vts_upstream_server_up{upstream=\"backend\",server=\"127.0.0.1:8080\"} 1"));
        
        
        // Verify basic VTS info is present
        assert!(status_content.contains("# nginx-vts-rust"));
        assert!(status_content.contains("# VTS Status: Active"));
        
        // The key validation: ensure that Prometheus metrics section is not empty
        // This was the main issue in ISSUE1.md
        assert!(status_content.contains("# HELP nginx_vts_upstream_requests_total Total upstream requests"));
        assert!(status_content.contains("# TYPE nginx_vts_upstream_requests_total counter"));
    }
}