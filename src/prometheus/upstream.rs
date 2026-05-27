//! `nginx_vts_upstream_*` series: requests, bytes, response_seconds
//! summary, server_up gauge, status counters, and the
//! `response_duration_seconds` classic histogram (compatible with
//! `histogram_quantile()` for p50/p90/p99 panels).

use std::collections::HashMap;

use super::PrometheusFormatter;
use crate::upstream_stats::{UpstreamZone, RESPONSE_TIME_BUCKET_BOUNDS_MS};

impl PrometheusFormatter {
    /// Format upstream statistics into Prometheus metrics.
    ///
    /// Generates metrics for upstream servers including request counts,
    /// byte transfers, response times, status code class counts, and
    /// the response-duration histogram.
    #[allow(dead_code)] // Used in tests and VTS integration
    pub fn format_upstream_stats(&self, upstream_zones: &HashMap<String, UpstreamZone>) -> String {
        let mut output = String::new();
        if upstream_zones.is_empty() {
            return output;
        }
        let prefix = &self.metric_prefix;

        // nginx_vts_upstream_requests_total
        output.push_str(&format!(
            "# HELP {prefix}upstream_requests_total Total upstream requests\n"
        ));
        output.push_str(&format!("# TYPE {prefix}upstream_requests_total counter\n"));
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                output.push_str(&format!(
                    "{prefix}upstream_requests_total{{upstream=\"{upstream_name}\",server=\"{server_addr}\"}} {}\n",
                    stats.request_counter
                ));
            }
        }
        output.push('\n');

        // nginx_vts_upstream_bytes_total
        output.push_str(&format!(
            "# HELP {prefix}upstream_bytes_total Total bytes transferred to/from upstream\n"
        ));
        output.push_str(&format!("# TYPE {prefix}upstream_bytes_total counter\n"));
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                output.push_str(&format!(
                    "{prefix}upstream_bytes_total{{upstream=\"{upstream_name}\",server=\"{server_addr}\",direction=\"in\"}} {}\n",
                    stats.in_bytes
                ));
                output.push_str(&format!(
                    "{prefix}upstream_bytes_total{{upstream=\"{upstream_name}\",server=\"{server_addr}\",direction=\"out\"}} {}\n",
                    stats.out_bytes
                ));
            }
        }
        output.push('\n');

        // nginx_vts_upstream_response_seconds (avg/total summary).
        output.push_str(&format!(
            "# HELP {prefix}upstream_response_seconds Upstream response time statistics\n"
        ));
        output.push_str(&format!("# TYPE {prefix}upstream_response_seconds gauge\n"));
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                let avg_request_time = stats.avg_request_time() / 1000.0;
                let avg_response_time = stats.avg_response_time() / 1000.0;
                let total_request_time = stats.request_time_total as f64 / 1000.0;
                let total_upstream_time = stats.response_time_total as f64 / 1000.0;
                for (kind, value) in [
                    ("request_avg", avg_request_time),
                    ("upstream_avg", avg_response_time),
                    ("request_total", total_request_time),
                    ("upstream_total", total_upstream_time),
                ] {
                    output.push_str(&format!(
                        "{prefix}upstream_response_seconds{{upstream=\"{upstream_name}\",server=\"{server_addr}\",type=\"{kind}\"}} {value:.6}\n"
                    ));
                }
            }
        }
        output.push('\n');

        // nginx_vts_upstream_server_up
        output.push_str(&format!(
            "# HELP {prefix}upstream_server_up Upstream server status (1=up, 0=down)\n"
        ));
        output.push_str(&format!("# TYPE {prefix}upstream_server_up gauge\n"));
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                let server_up = if stats.down { 0 } else { 1 };
                output.push_str(&format!(
                    "{prefix}upstream_server_up{{upstream=\"{upstream_name}\",server=\"{server_addr}\"}} {server_up}\n"
                ));
            }
        }
        output.push('\n');

        // HTTP status code metrics and response-time histogram.
        self.format_upstream_status_metrics(&mut output, upstream_zones);
        self.format_upstream_response_histogram(&mut output, upstream_zones);

        output
    }

    /// `nginx_vts_upstream_responses_total{status="1xx"…"5xx"}` (class buckets).
    #[allow(dead_code)] // Used in format_upstream_stats method
    fn format_upstream_status_metrics(
        &self,
        output: &mut String,
        upstream_zones: &HashMap<String, UpstreamZone>,
    ) {
        let prefix = &self.metric_prefix;
        output.push_str(&format!(
            "# HELP {prefix}upstream_responses_total Upstream responses by status code\n"
        ));
        output.push_str(&format!(
            "# TYPE {prefix}upstream_responses_total counter\n"
        ));
        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                for (class, value) in [
                    ("1xx", stats.responses.status_1xx),
                    ("2xx", stats.responses.status_2xx),
                    ("3xx", stats.responses.status_3xx),
                    ("4xx", stats.responses.status_4xx),
                    ("5xx", stats.responses.status_5xx),
                ] {
                    output.push_str(&format!(
                        "{prefix}upstream_responses_total{{upstream=\"{upstream_name}\",server=\"{server_addr}\",status=\"{class}\"}} {value}\n"
                    ));
                }
            }
        }
        output.push('\n');
    }

    /// `nginx_vts_upstream_response_duration_seconds` classic
    /// histogram (`_bucket{le="..."}`, `_sum`, `_count`).  Compatible
    /// with `histogram_quantile()` for p50 / p90 / p99 panels.
    #[allow(dead_code)] // Used in format_upstream_stats method
    fn format_upstream_response_histogram(
        &self,
        output: &mut String,
        upstream_zones: &HashMap<String, UpstreamZone>,
    ) {
        let prefix = &self.metric_prefix;
        output.push_str(&format!(
            "# HELP {prefix}upstream_response_duration_seconds Upstream response time distribution\n"
        ));
        output.push_str(&format!(
            "# TYPE {prefix}upstream_response_duration_seconds histogram\n"
        ));

        for (upstream_name, zone) in upstream_zones {
            for (server_addr, stats) in &zone.servers {
                for (i, &bound_ms) in RESPONSE_TIME_BUCKET_BOUNDS_MS.iter().enumerate() {
                    let bound_s = bound_ms as f64 / 1000.0;
                    output.push_str(&format!(
                        "{prefix}upstream_response_duration_seconds_bucket{{upstream=\"{upstream_name}\",server=\"{server_addr}\",le=\"{}\"}} {}\n",
                        format_le_bound(bound_s),
                        stats.response_buckets[i]
                    ));
                }
                // +Inf bucket holds every sample, equal to _count.
                output.push_str(&format!(
                    "{prefix}upstream_response_duration_seconds_bucket{{upstream=\"{upstream_name}\",server=\"{server_addr}\",le=\"+Inf\"}} {}\n",
                    stats.response_time_counter
                ));
                output.push_str(&format!(
                    "{prefix}upstream_response_duration_seconds_sum{{upstream=\"{upstream_name}\",server=\"{server_addr}\"}} {:.6}\n",
                    stats.response_time_total as f64 / 1000.0
                ));
                output.push_str(&format!(
                    "{prefix}upstream_response_duration_seconds_count{{upstream=\"{upstream_name}\",server=\"{server_addr}\"}} {}\n",
                    stats.response_time_counter
                ));
            }
        }
        output.push('\n');
    }
}

/// Format a histogram `le` bound (in seconds) as Prometheus expects
/// — fixed-point with trailing zeros trimmed: `0.005`, `0.01`,
/// `0.1`, `1`, `2.5`, `10`.  The rendering must be stable across
/// scrapes so the time series doesn't fragment.
fn format_le_bound(seconds: f64) -> String {
    let formatted = format!("{seconds:.3}");
    let trimmed = formatted.trim_end_matches('0').trim_end_matches('.');
    if trimmed.is_empty() {
        "0".to_string()
    } else {
        trimmed.to_string()
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
        server1.request_time_total = 5000;
        server1.request_time_counter = 100;
        server1.response_time_total = 2500;
        server1.response_time_counter = 100;
        server1.response_buckets = [10, 20, 35, 60, 80, 95, 98, 99, 100, 100, 100];
        server1.responses.status_2xx = 95;
        server1.responses.status_4xx = 3;
        server1.responses.status_5xx = 2;
        server1.down = false;

        let mut server2 = UpstreamServerStats::new("10.0.0.2:80");
        server2.request_counter = 50;
        server2.in_bytes = 25000;
        server2.out_bytes = 12500;
        server2.down = true;

        zone.servers.insert("10.0.0.1:80".to_string(), server1);
        zone.servers.insert("10.0.0.2:80".to_string(), server2);
        zone
    }

    #[test]
    fn format_le_bound_trims_trailing_zeros() {
        assert_eq!(format_le_bound(0.005), "0.005");
        assert_eq!(format_le_bound(0.01), "0.01");
        assert_eq!(format_le_bound(0.1), "0.1");
        assert_eq!(format_le_bound(1.0), "1");
        assert_eq!(format_le_bound(2.5), "2.5");
        assert_eq!(format_le_bound(10.0), "10");
    }

    #[test]
    fn empty_upstream_zones_render_to_empty_string() {
        let f = PrometheusFormatter::new();
        let empty: HashMap<String, UpstreamZone> = HashMap::new();
        assert!(f.format_upstream_stats(&empty).is_empty());
    }

    #[test]
    fn upstream_stats_render_all_families() {
        let mut zones = HashMap::new();
        zones.insert("test_backend".to_string(), create_test_upstream_zone());
        let out = PrometheusFormatter::new().format_upstream_stats(&zones);

        // Counter / bytes / response_seconds / server_up.
        assert!(out.contains("# HELP nginx_vts_upstream_requests_total"));
        assert!(out.contains("nginx_vts_upstream_requests_total{upstream=\"test_backend\",server=\"10.0.0.1:80\"} 100"));
        assert!(out.contains("nginx_vts_upstream_requests_total{upstream=\"test_backend\",server=\"10.0.0.2:80\"} 50"));
        assert!(out.contains("nginx_vts_upstream_bytes_total{upstream=\"test_backend\",server=\"10.0.0.1:80\",direction=\"in\"} 50000"));
        assert!(out.contains("nginx_vts_upstream_bytes_total{upstream=\"test_backend\",server=\"10.0.0.1:80\",direction=\"out\"} 25000"));
        assert!(out.contains(
            "nginx_vts_upstream_server_up{upstream=\"test_backend\",server=\"10.0.0.1:80\"} 1"
        ));
        assert!(out.contains(
            "nginx_vts_upstream_server_up{upstream=\"test_backend\",server=\"10.0.0.2:80\"} 0"
        ));
        assert!(out.contains("nginx_vts_upstream_response_seconds{upstream=\"test_backend\",server=\"10.0.0.1:80\",type=\"request_avg\"} 0.050000"));
        assert!(out.contains("nginx_vts_upstream_response_seconds{upstream=\"test_backend\",server=\"10.0.0.1:80\",type=\"upstream_avg\"} 0.025000"));

        // Histogram.
        assert!(out.contains("# HELP nginx_vts_upstream_response_duration_seconds Upstream response time distribution"));
        assert!(out.contains("# TYPE nginx_vts_upstream_response_duration_seconds histogram"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_bucket{upstream=\"test_backend\",server=\"10.0.0.1:80\",le=\"0.005\"} 10"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_bucket{upstream=\"test_backend\",server=\"10.0.0.1:80\",le=\"0.1\"} 80"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_bucket{upstream=\"test_backend\",server=\"10.0.0.1:80\",le=\"1\"} 99"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_bucket{upstream=\"test_backend\",server=\"10.0.0.1:80\",le=\"+Inf\"} 100"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_sum{upstream=\"test_backend\",server=\"10.0.0.1:80\"} 2.500000"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_count{upstream=\"test_backend\",server=\"10.0.0.1:80\"} 100"));
    }

    #[test]
    fn custom_prefix_replaces_default_throughout() {
        let f = PrometheusFormatter::with_prefix("custom_vts_");
        let mut zones = HashMap::new();
        zones.insert("test_backend".to_string(), create_test_upstream_zone());
        let out = f.format_upstream_stats(&zones);
        assert!(out.contains("# HELP custom_vts_upstream_requests_total"));
        assert!(out.contains("custom_vts_upstream_requests_total{upstream=\"test_backend\""));
        assert!(!out.contains("nginx_vts_"));
    }
}
