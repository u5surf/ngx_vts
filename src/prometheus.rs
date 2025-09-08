//! Prometheus metrics formatting module for VTS
//!
//! This module provides functionality to format VTS statistics into Prometheus
//! metrics format, including upstream server statistics, cache statistics,
//! and general server zone metrics.

use std::collections::HashMap;
use crate::upstream_stats::UpstreamZone;
use crate::cache_stats::{CacheZoneStats};

/// Prometheus metrics formatter for VTS statistics
///
/// Formats various VTS statistics into Prometheus metrics format with
/// proper metric names, labels, and help text according to Prometheus
/// best practices.
pub struct PrometheusFormatter {
    /// Optional metric prefix (default: "nginx_vts_")
    pub metric_prefix: String,
}

impl PrometheusFormatter {
    /// Create a new Prometheus formatter with default settings
    pub fn new() -> Self {
        Self {
            metric_prefix: "nginx_vts_".to_string(),
        }
    }
    
    /// Create a new Prometheus formatter with custom metric prefix
    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            metric_prefix: prefix.to_string(),
        }
    }
    
    /// Format upstream statistics into Prometheus metrics
    ///
    /// Generates metrics for upstream servers including request counts,
    /// byte transfers, response times, and server status.
    ///
    /// # Arguments
    ///
    /// * `upstream_zones` - HashMap of upstream zones with their statistics
    ///
    /// # Returns
    ///
    /// String containing formatted Prometheus metrics
    pub fn format_upstream_stats(&self, upstream_zones: &HashMap<String, UpstreamZone>) -> String {
        let mut output = String::new();
        
        if upstream_zones.is_empty() {
            return output;
        }

        // nginx_vts_upstream_requests_total
        output.push_str(&format!("# HELP {}upstream_requests_total Total upstream requests\n", self.metric_prefix));
        output.push_str(&format!("# TYPE {}upstream_requests_total counter\n", self.metric_prefix));
        
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                output.push_str(&format!(
                    "{}upstream_requests_total{{upstream=\"{}\",server=\"{}\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, stats.request_counter
                ));
            }
        }
        output.push('\n');

        // nginx_vts_upstream_bytes_total
        output.push_str(&format!("# HELP {}upstream_bytes_total Total bytes transferred to/from upstream\n", self.metric_prefix));
        output.push_str(&format!("# TYPE {}upstream_bytes_total counter\n", self.metric_prefix));
        
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                // Bytes received from upstream (in_bytes)
                output.push_str(&format!(
                    "{}upstream_bytes_total{{upstream=\"{}\",server=\"{}\",direction=\"in\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, stats.in_bytes
                ));
                // Bytes sent to upstream (out_bytes)
                output.push_str(&format!(
                    "{}upstream_bytes_total{{upstream=\"{}\",server=\"{}\",direction=\"out\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, stats.out_bytes
                ));
            }
        }
        output.push('\n');

        // nginx_vts_upstream_response_seconds
        output.push_str(&format!("# HELP {}upstream_response_seconds Upstream response time statistics\n", self.metric_prefix));
        output.push_str(&format!("# TYPE {}upstream_response_seconds gauge\n", self.metric_prefix));
        
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                // Average request time
                let avg_request_time = stats.avg_request_time() / 1000.0; // Convert ms to seconds
                output.push_str(&format!(
                    "{}upstream_response_seconds{{upstream=\"{}\",server=\"{}\",type=\"request_avg\"}} {:.6}\n",
                    self.metric_prefix, upstream_name, server_addr, avg_request_time
                ));
                
                // Average upstream response time
                let avg_response_time = stats.avg_response_time() / 1000.0; // Convert ms to seconds
                output.push_str(&format!(
                    "{}upstream_response_seconds{{upstream=\"{}\",server=\"{}\",type=\"upstream_avg\"}} {:.6}\n",
                    self.metric_prefix, upstream_name, server_addr, avg_response_time
                ));
                
                // Total request time
                let total_request_time = stats.request_time_total as f64 / 1000.0; // Convert ms to seconds
                output.push_str(&format!(
                    "{}upstream_response_seconds{{upstream=\"{}\",server=\"{}\",type=\"request_total\"}} {:.6}\n",
                    self.metric_prefix, upstream_name, server_addr, total_request_time
                ));
                
                // Total upstream response time
                let total_upstream_time = stats.response_time_total as f64 / 1000.0; // Convert ms to seconds
                output.push_str(&format!(
                    "{}upstream_response_seconds{{upstream=\"{}\",server=\"{}\",type=\"upstream_total\"}} {:.6}\n",
                    self.metric_prefix, upstream_name, server_addr, total_upstream_time
                ));
            }
        }
        output.push('\n');

        // nginx_vts_upstream_server_up
        output.push_str(&format!("# HELP {}upstream_server_up Upstream server status (1=up, 0=down)\n", self.metric_prefix));
        output.push_str(&format!("# TYPE {}upstream_server_up gauge\n", self.metric_prefix));
        
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                let server_up = if stats.down { 0 } else { 1 };
                output.push_str(&format!(
                    "{}upstream_server_up{{upstream=\"{}\",server=\"{}\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, server_up
                ));
            }
        }
        output.push('\n');

        // HTTP status code metrics
        self.format_upstream_status_metrics(&mut output, upstream_zones);

        output
    }

    /// Format upstream HTTP status code metrics
    fn format_upstream_status_metrics(&self, output: &mut String, upstream_zones: &HashMap<String, UpstreamZone>) {
        output.push_str(&format!("# HELP {}upstream_responses_total Upstream responses by status code\n", self.metric_prefix));
        output.push_str(&format!("# TYPE {}upstream_responses_total counter\n", self.metric_prefix));

        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                // 1xx responses
                if stats.responses.status_1xx > 0 {
                    output.push_str(&format!(
                        "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"1xx\"}} {}\n",
                        self.metric_prefix, upstream_name, server_addr, stats.responses.status_1xx
                    ));
                }
                
                // 2xx responses
                if stats.responses.status_2xx > 0 {
                    output.push_str(&format!(
                        "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"2xx\"}} {}\n",
                        self.metric_prefix, upstream_name, server_addr, stats.responses.status_2xx
                    ));
                }
                
                // 3xx responses
                if stats.responses.status_3xx > 0 {
                    output.push_str(&format!(
                        "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"3xx\"}} {}\n",
                        self.metric_prefix, upstream_name, server_addr, stats.responses.status_3xx
                    ));
                }
                
                // 4xx responses
                if stats.responses.status_4xx > 0 {
                    output.push_str(&format!(
                        "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"4xx\"}} {}\n",
                        self.metric_prefix, upstream_name, server_addr, stats.responses.status_4xx
                    ));
                }
                
                // 5xx responses
                if stats.responses.status_5xx > 0 {
                    output.push_str(&format!(
                        "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"5xx\"}} {}\n",
                        self.metric_prefix, upstream_name, server_addr, stats.responses.status_5xx
                    ));
                }
            }
        }
        output.push('\n');
    }

    /// Format cache zone statistics into Prometheus metrics
    ///
    /// Generates metrics for cache zones including hit ratios, cache size,
    /// and cache response statistics.
    ///
    /// # Arguments
    ///
    /// * `cache_zones` - HashMap of cache zones with their statistics
    ///
    /// # Returns
    ///
    /// String containing formatted Prometheus cache metrics
    pub fn format_cache_stats(&self, cache_zones: &HashMap<String, CacheZoneStats>) -> String {
        let mut output = String::new();
        
        if cache_zones.is_empty() {
            return output;
        }

        // nginx_vts_cache_size_bytes
        output.push_str(&format!("# HELP {}cache_size_bytes Cache size in bytes\n", self.metric_prefix));
        output.push_str(&format!("# TYPE {}cache_size_bytes gauge\n", self.metric_prefix));
        
        for (zone_name, cache_stats) in cache_zones {
            // Maximum cache size
            output.push_str(&format!(
                "{}cache_size_bytes{{zone=\"{}\",type=\"max\"}} {}\n",
                self.metric_prefix, zone_name, cache_stats.max_size
            ));
            
            // Used cache size
            output.push_str(&format!(
                "{}cache_size_bytes{{zone=\"{}\",type=\"used\"}} {}\n",
                self.metric_prefix, zone_name, cache_stats.used_size
            ));
        }
        output.push('\n');

        // nginx_vts_cache_hits_total
        output.push_str(&format!("# HELP {}cache_hits_total Cache hit statistics\n", self.metric_prefix));
        output.push_str(&format!("# TYPE {}cache_hits_total counter\n", self.metric_prefix));
        
        for (zone_name, cache_stats) in cache_zones {
            let responses = &cache_stats.responses;
            
            output.push_str(&format!(
                "{}cache_hits_total{{zone=\"{}\",status=\"hit\"}} {}\n",
                self.metric_prefix, zone_name, responses.hit
            ));
            output.push_str(&format!(
                "{}cache_hits_total{{zone=\"{}\",status=\"miss\"}} {}\n",
                self.metric_prefix, zone_name, responses.miss
            ));
            output.push_str(&format!(
                "{}cache_hits_total{{zone=\"{}\",status=\"bypass\"}} {}\n",
                self.metric_prefix, zone_name, responses.bypass
            ));
            output.push_str(&format!(
                "{}cache_hits_total{{zone=\"{}\",status=\"expired\"}} {}\n",
                self.metric_prefix, zone_name, responses.expired
            ));
            output.push_str(&format!(
                "{}cache_hits_total{{zone=\"{}\",status=\"stale\"}} {}\n",
                self.metric_prefix, zone_name, responses.stale
            ));
            output.push_str(&format!(
                "{}cache_hits_total{{zone=\"{}\",status=\"updating\"}} {}\n",
                self.metric_prefix, zone_name, responses.updating
            ));
            output.push_str(&format!(
                "{}cache_hits_total{{zone=\"{}\",status=\"revalidated\"}} {}\n",
                self.metric_prefix, zone_name, responses.revalidated
            ));
            output.push_str(&format!(
                "{}cache_hits_total{{zone=\"{}\",status=\"scarce\"}} {}\n",
                self.metric_prefix, zone_name, responses.scarce
            ));
        }
        output.push('\n');

        output
    }

    /// Format complete VTS metrics including upstream and cache statistics
    ///
    /// # Arguments
    ///
    /// * `upstream_zones` - Upstream zones statistics
    /// * `cache_zones` - Cache zones statistics
    ///
    /// # Returns
    ///
    /// String containing all formatted Prometheus metrics
    pub fn format_all_stats(
        &self,
        upstream_zones: &HashMap<String, UpstreamZone>,
        cache_zones: &HashMap<String, CacheZoneStats>,
    ) -> String {
        let mut output = String::new();
        
        // Add upstream metrics
        if !upstream_zones.is_empty() {
            output.push_str(&self.format_upstream_stats(upstream_zones));
        }
        
        // Add cache metrics
        if !cache_zones.is_empty() {
            output.push_str(&self.format_cache_stats(cache_zones));
        }
        
        output
    }
}

impl Default for PrometheusFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream_stats::{UpstreamZone, UpstreamServerStats};
    use crate::cache_stats::CacheZoneStats;

    fn create_test_upstream_zone() -> UpstreamZone {
        let mut zone = UpstreamZone::new("test_backend");
        
        let mut server1 = UpstreamServerStats::new("10.0.0.1:80");
        server1.request_counter = 100;
        server1.in_bytes = 50000;
        server1.out_bytes = 25000;
        server1.request_time_total = 5000; // 5 seconds total
        server1.request_time_counter = 100;
        server1.response_time_total = 2500; // 2.5 seconds total
        server1.response_time_counter = 100;
        server1.responses.status_2xx = 95;
        server1.responses.status_4xx = 3;
        server1.responses.status_5xx = 2;
        server1.down = false;
        
        let mut server2 = UpstreamServerStats::new("10.0.0.2:80");
        server2.request_counter = 50;
        server2.in_bytes = 25000;
        server2.out_bytes = 12500;
        server2.down = true; // This server is down
        
        zone.servers.insert("10.0.0.1:80".to_string(), server1);
        zone.servers.insert("10.0.0.2:80".to_string(), server2);
        
        zone
    }

    fn create_test_cache_zone() -> CacheZoneStats {
        let mut cache = CacheZoneStats::new("test_cache", 1073741824); // 1GB max
        cache.used_size = 536870912; // 512MB used
        cache.in_bytes = 1000000; // 1MB read from cache
        cache.out_bytes = 500000; // 500KB written to cache
        
        cache.responses.hit = 800;
        cache.responses.miss = 150;
        cache.responses.expired = 30;
        cache.responses.bypass = 20;
        
        cache
    }

    #[test]
    fn test_prometheus_formatter_creation() {
        let formatter = PrometheusFormatter::new();
        assert_eq!(formatter.metric_prefix, "nginx_vts_");
        
        let custom_formatter = PrometheusFormatter::with_prefix("custom_");
        assert_eq!(custom_formatter.metric_prefix, "custom_");
    }

    #[test]
    fn test_format_upstream_stats() {
        let formatter = PrometheusFormatter::new();
        let mut upstream_zones = HashMap::new();
        upstream_zones.insert("test_backend".to_string(), create_test_upstream_zone());
        
        let output = formatter.format_upstream_stats(&upstream_zones);
        
        // Verify basic structure
        assert!(output.contains("# HELP nginx_vts_upstream_requests_total"));
        assert!(output.contains("# TYPE nginx_vts_upstream_requests_total counter"));
        
        // Verify request metrics
        assert!(output.contains("nginx_vts_upstream_requests_total{upstream=\"test_backend\",server=\"10.0.0.1:80\"} 100"));
        assert!(output.contains("nginx_vts_upstream_requests_total{upstream=\"test_backend\",server=\"10.0.0.2:80\"} 50"));
        
        // Verify byte metrics
        assert!(output.contains("nginx_vts_upstream_bytes_total{upstream=\"test_backend\",server=\"10.0.0.1:80\",direction=\"in\"} 50000"));
        assert!(output.contains("nginx_vts_upstream_bytes_total{upstream=\"test_backend\",server=\"10.0.0.1:80\",direction=\"out\"} 25000"));
        
        // Verify server status
        assert!(output.contains("nginx_vts_upstream_server_up{upstream=\"test_backend\",server=\"10.0.0.1:80\"} 1"));
        assert!(output.contains("nginx_vts_upstream_server_up{upstream=\"test_backend\",server=\"10.0.0.2:80\"} 0"));
        
        // Verify response time metrics (should be in seconds, not milliseconds)
        assert!(output.contains("nginx_vts_upstream_response_seconds{upstream=\"test_backend\",server=\"10.0.0.1:80\",type=\"request_avg\"} 0.050000")); // 50ms avg -> 0.05s
        assert!(output.contains("nginx_vts_upstream_response_seconds{upstream=\"test_backend\",server=\"10.0.0.1:80\",type=\"upstream_avg\"} 0.025000")); // 25ms avg -> 0.025s
    }

    #[test]
    fn test_format_cache_stats() {
        let formatter = PrometheusFormatter::new();
        let mut cache_zones = HashMap::new();
        cache_zones.insert("test_cache".to_string(), create_test_cache_zone());
        
        let output = formatter.format_cache_stats(&cache_zones);
        
        // Verify cache size metrics
        assert!(output.contains("# HELP nginx_vts_cache_size_bytes"));
        assert!(output.contains("nginx_vts_cache_size_bytes{zone=\"test_cache\",type=\"max\"} 1073741824"));
        assert!(output.contains("nginx_vts_cache_size_bytes{zone=\"test_cache\",type=\"used\"} 536870912"));
        
        // Verify cache hit metrics
        assert!(output.contains("# HELP nginx_vts_cache_hits_total"));
        assert!(output.contains("nginx_vts_cache_hits_total{zone=\"test_cache\",status=\"hit\"} 800"));
        assert!(output.contains("nginx_vts_cache_hits_total{zone=\"test_cache\",status=\"miss\"} 150"));
    }

    #[test]
    fn test_format_empty_stats() {
        let formatter = PrometheusFormatter::new();
        let empty_upstream: HashMap<String, UpstreamZone> = HashMap::new();
        let empty_cache: HashMap<String, CacheZoneStats> = HashMap::new();
        
        let upstream_output = formatter.format_upstream_stats(&empty_upstream);
        let cache_output = formatter.format_cache_stats(&empty_cache);
        
        assert!(upstream_output.is_empty());
        assert!(cache_output.is_empty());
    }

    #[test]
    fn test_format_all_stats() {
        let formatter = PrometheusFormatter::new();
        let mut upstream_zones = HashMap::new();
        let mut cache_zones = HashMap::new();
        
        upstream_zones.insert("test_backend".to_string(), create_test_upstream_zone());
        cache_zones.insert("test_cache".to_string(), create_test_cache_zone());
        
        let output = formatter.format_all_stats(&upstream_zones, &cache_zones);
        
        // Should contain both upstream and cache metrics
        assert!(output.contains("nginx_vts_upstream_requests_total"));
        assert!(output.contains("nginx_vts_cache_size_bytes"));
        assert!(output.contains("nginx_vts_cache_hits_total"));
    }

    #[test]
    fn test_custom_metric_prefix() {
        let formatter = PrometheusFormatter::with_prefix("custom_vts_");
        let mut upstream_zones = HashMap::new();
        upstream_zones.insert("test_backend".to_string(), create_test_upstream_zone());
        
        let output = formatter.format_upstream_stats(&upstream_zones);
        
        // Verify custom prefix is used
        assert!(output.contains("# HELP custom_vts_upstream_requests_total"));
        assert!(output.contains("custom_vts_upstream_requests_total{upstream=\"test_backend\""));
        assert!(!output.contains("nginx_vts_")); // Should not contain default prefix
    }
}