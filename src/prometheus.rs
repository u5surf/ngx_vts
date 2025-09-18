//! Prometheus metrics formatting module for VTS
//!
//! This module provides functionality to format VTS statistics into Prometheus
//! metrics format, including upstream server statistics, cache statistics,
//! and general server zone metrics.

use crate::stats::{VtsConnectionStats, VtsServerStats};
use crate::upstream_stats::UpstreamZone;
use std::collections::HashMap;

/// Prometheus metrics formatter for VTS statistics
///
/// Formats various VTS statistics into Prometheus metrics format with
/// proper metric names, labels, and help text according to Prometheus
/// best practices.
#[allow(dead_code)] // All fields used in formatting
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
    #[allow(dead_code)] // Used in tests and future integrations
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
    #[allow(dead_code)] // Used in tests and VTS integration
    pub fn format_upstream_stats(&self, upstream_zones: &HashMap<String, UpstreamZone>) -> String {
        let mut output = String::new();

        if upstream_zones.is_empty() {
            return output;
        }

        // nginx_vts_upstream_requests_total
        output.push_str(&format!(
            "# HELP {}upstream_requests_total Total upstream requests\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}upstream_requests_total counter\n",
            self.metric_prefix
        ));

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
        output.push_str(&format!(
            "# HELP {}upstream_bytes_total Total bytes transferred to/from upstream\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}upstream_bytes_total counter\n",
            self.metric_prefix
        ));

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
        output.push_str(&format!(
            "# HELP {}upstream_response_seconds Upstream response time statistics\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}upstream_response_seconds gauge\n",
            self.metric_prefix
        ));

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
        output.push_str(&format!(
            "# HELP {}upstream_server_up Upstream server status (1=up, 0=down)\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}upstream_server_up gauge\n",
            self.metric_prefix
        ));

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
    #[allow(dead_code)] // Used in format_upstream_stats method
    fn format_upstream_status_metrics(
        &self,
        output: &mut String,
        upstream_zones: &HashMap<String, UpstreamZone>,
    ) {
        output.push_str(&format!(
            "# HELP {}upstream_responses_total Upstream responses by status code\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}upstream_responses_total counter\n",
            self.metric_prefix
        ));

        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                // Always show status code metrics, even when 0 (for proper VTS initialization display)

                // 1xx responses
                output.push_str(&format!(
                    "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"1xx\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, stats.responses.status_1xx
                ));

                // 2xx responses
                output.push_str(&format!(
                    "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"2xx\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, stats.responses.status_2xx
                ));

                // 3xx responses
                output.push_str(&format!(
                    "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"3xx\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, stats.responses.status_3xx
                ));

                // 4xx responses
                output.push_str(&format!(
                    "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"4xx\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, stats.responses.status_4xx
                ));

                // 5xx responses
                output.push_str(&format!(
                    "{}upstream_responses_total{{upstream=\"{}\",server=\"{}\",status=\"5xx\"}} {}\n",
                    self.metric_prefix, upstream_name, server_addr, stats.responses.status_5xx
                ));
            }
        }
        output.push('\n');
    }

    /// Format nginx basic info metrics into Prometheus format
    pub fn format_nginx_info(&self, hostname: &str, version: &str) -> String {
        let mut output = String::new();

        output.push_str(&format!(
            "# HELP {}info Nginx VTS module information\n",
            self.metric_prefix
        ));
        output.push_str(&format!("# TYPE {}info gauge\n", self.metric_prefix));
        output.push_str(&format!(
            "{}info{{hostname=\"{}\",version=\"{}\"}} 1\n\n",
            self.metric_prefix, hostname, version
        ));

        output
    }

    /// Format connection statistics into Prometheus metrics
    pub fn format_connection_stats(&self, connections: &VtsConnectionStats) -> String {
        let mut output = String::new();

        // Current connections
        output.push_str(&format!(
            "# HELP {}connections Current nginx connections\n",
            self.metric_prefix
        ));
        output.push_str(&format!("# TYPE {}connections gauge\n", self.metric_prefix));
        output.push_str(&format!(
            "{}connections{{state=\"active\"}} {}\n",
            self.metric_prefix, connections.active
        ));
        output.push_str(&format!(
            "{}connections{{state=\"reading\"}} {}\n",
            self.metric_prefix, connections.reading
        ));
        output.push_str(&format!(
            "{}connections{{state=\"writing\"}} {}\n",
            self.metric_prefix, connections.writing
        ));
        output.push_str(&format!(
            "{}connections{{state=\"waiting\"}} {}\n",
            self.metric_prefix, connections.waiting
        ));
        output.push('\n');

        // Total connections
        output.push_str(&format!(
            "# HELP {}connections_total Total nginx connections\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}connections_total counter\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "{}connections_total{{state=\"accepted\"}} {}\n",
            self.metric_prefix, connections.accepted
        ));
        output.push_str(&format!(
            "{}connections_total{{state=\"handled\"}} {}\n",
            self.metric_prefix, connections.handled
        ));
        output.push('\n');

        output
    }

    /// Format server zone statistics into Prometheus metrics
    pub fn format_server_stats(&self, server_stats: &HashMap<String, VtsServerStats>) -> String {
        let mut output = String::new();

        // Server requests total
        output.push_str(&format!(
            "# HELP {}server_requests_total Total number of requests\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}server_requests_total counter\n",
            self.metric_prefix
        ));
        for (zone, stats) in server_stats {
            output.push_str(&format!(
                "{}server_requests_total{{zone=\"{}\"}} {}\n",
                self.metric_prefix, zone, stats.requests
            ));
        }
        output.push('\n');

        // Server bytes total
        output.push_str(&format!(
            "# HELP {}server_bytes_total Total bytes transferred\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}server_bytes_total counter\n",
            self.metric_prefix
        ));
        for (zone, stats) in server_stats {
            output.push_str(&format!(
                "{}server_bytes_total{{zone=\"{}\",direction=\"in\"}} {}\n",
                self.metric_prefix, zone, stats.bytes_in
            ));
            output.push_str(&format!(
                "{}server_bytes_total{{zone=\"{}\",direction=\"out\"}} {}\n",
                self.metric_prefix, zone, stats.bytes_out
            ));
        }
        output.push('\n');

        // Server responses total
        output.push_str(&format!(
            "# HELP {}server_responses_total Total responses by status code\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}server_responses_total counter\n",
            self.metric_prefix
        ));
        for (zone, stats) in server_stats {
            output.push_str(&format!(
                "{}server_responses_total{{zone=\"{}\",status=\"1xx\"}} {}\n",
                self.metric_prefix, zone, stats.responses.status_1xx
            ));
            output.push_str(&format!(
                "{}server_responses_total{{zone=\"{}\",status=\"2xx\"}} {}\n",
                self.metric_prefix, zone, stats.responses.status_2xx
            ));
            output.push_str(&format!(
                "{}server_responses_total{{zone=\"{}\",status=\"3xx\"}} {}\n",
                self.metric_prefix, zone, stats.responses.status_3xx
            ));
            output.push_str(&format!(
                "{}server_responses_total{{zone=\"{}\",status=\"4xx\"}} {}\n",
                self.metric_prefix, zone, stats.responses.status_4xx
            ));
            output.push_str(&format!(
                "{}server_responses_total{{zone=\"{}\",status=\"5xx\"}} {}\n",
                self.metric_prefix, zone, stats.responses.status_5xx
            ));
        }
        output.push('\n');

        // Server request seconds
        output.push_str(&format!(
            "# HELP {}server_request_seconds Request processing time\n",
            self.metric_prefix
        ));
        output.push_str(&format!(
            "# TYPE {}server_request_seconds gauge\n",
            self.metric_prefix
        ));
        for (zone, stats) in server_stats {
            output.push_str(&format!(
                "{}server_request_seconds{{zone=\"{}\",type=\"avg\"}} {:.6}\n",
                self.metric_prefix, zone, stats.request_times.avg
            ));
            output.push_str(&format!(
                "{}server_request_seconds{{zone=\"{}\",type=\"min\"}} {:.6}\n",
                self.metric_prefix, zone, stats.request_times.min
            ));
            output.push_str(&format!(
                "{}server_request_seconds{{zone=\"{}\",type=\"max\"}} {:.6}\n",
                self.metric_prefix, zone, stats.request_times.max
            ));
        }
        output.push('\n');

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
    use crate::upstream_stats::{UpstreamServerStats, UpstreamZone};

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
        assert!(output.contains(
            "nginx_vts_upstream_server_up{upstream=\"test_backend\",server=\"10.0.0.1:80\"} 1"
        ));
        assert!(output.contains(
            "nginx_vts_upstream_server_up{upstream=\"test_backend\",server=\"10.0.0.2:80\"} 0"
        ));

        // Verify response time metrics (should be in seconds, not milliseconds)
        assert!(output.contains("nginx_vts_upstream_response_seconds{upstream=\"test_backend\",server=\"10.0.0.1:80\",type=\"request_avg\"} 0.050000")); // 50ms avg -> 0.05s
        assert!(output.contains("nginx_vts_upstream_response_seconds{upstream=\"test_backend\",server=\"10.0.0.1:80\",type=\"upstream_avg\"} 0.025000"));
        // 25ms avg -> 0.025s
    }

    #[test]
    fn test_format_empty_stats() {
        let formatter = PrometheusFormatter::new();
        let empty_upstream: HashMap<String, UpstreamZone> = HashMap::new();

        let upstream_output = formatter.format_upstream_stats(&empty_upstream);

        assert!(upstream_output.is_empty());
    }

    #[test]
    fn test_format_upstream_only() {
        let formatter = PrometheusFormatter::new();
        let mut upstream_zones = HashMap::new();

        upstream_zones.insert("test_backend".to_string(), create_test_upstream_zone());

        let output = formatter.format_upstream_stats(&upstream_zones);

        // Should contain upstream metrics
        assert!(output.contains("nginx_vts_upstream_requests_total"));
        assert!(output.contains("nginx_vts_upstream_bytes_total"));
        assert!(output.contains("nginx_vts_upstream_response_seconds"));
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
