//! `nginx_vts_server_*` series — requests, bytes, responses,
//! `request_seconds` summary, and the
//! `request_duration_seconds` classic histogram.

use std::collections::HashMap;

use super::{format_le_bound, label, PrometheusFormatter};
use crate::stats::VtsServerStats;
use crate::upstream_stats::RESPONSE_TIME_BUCKET_BOUNDS_MS;

impl PrometheusFormatter {
    /// Format server zone statistics into Prometheus metrics.
    pub fn format_server_stats(&self, server_stats: &HashMap<String, VtsServerStats>) -> String {
        let mut output = String::new();
        let prefix = &self.metric_prefix;

        // Server requests total.
        output.push_str(&format!(
            "# HELP {prefix}server_requests_total Total number of requests\n"
        ));
        output.push_str(&format!("# TYPE {prefix}server_requests_total counter\n"));
        for (zone, stats) in server_stats {
            let zone = label::escape(zone);
            output.push_str(&format!(
                "{prefix}server_requests_total{{zone=\"{zone}\"}} {}\n",
                stats.requests
            ));
        }
        output.push('\n');

        // Server bytes total.
        output.push_str(&format!(
            "# HELP {prefix}server_bytes_total Total bytes transferred\n"
        ));
        output.push_str(&format!("# TYPE {prefix}server_bytes_total counter\n"));
        for (zone, stats) in server_stats {
            let zone = label::escape(zone);
            output.push_str(&format!(
                "{prefix}server_bytes_total{{zone=\"{zone}\",direction=\"in\"}} {}\n",
                stats.bytes_in
            ));
            output.push_str(&format!(
                "{prefix}server_bytes_total{{zone=\"{zone}\",direction=\"out\"}} {}\n",
                stats.bytes_out
            ));
        }
        output.push('\n');

        // Server responses total.
        output.push_str(&format!(
            "# HELP {prefix}server_responses_total Total responses by status code\n"
        ));
        output.push_str(&format!("# TYPE {prefix}server_responses_total counter\n"));
        for (zone, stats) in server_stats {
            let zone = label::escape(zone);
            for (class, value) in [
                ("1xx", stats.responses.status_1xx),
                ("2xx", stats.responses.status_2xx),
                ("3xx", stats.responses.status_3xx),
                ("4xx", stats.responses.status_4xx),
                ("5xx", stats.responses.status_5xx),
            ] {
                output.push_str(&format!(
                    "{prefix}server_responses_total{{zone=\"{zone}\",status=\"{class}\"}} {value}\n"
                ));
            }
        }
        output.push('\n');

        // Server request seconds (avg/min/max gauges).
        output.push_str(&format!(
            "# HELP {prefix}server_request_seconds Request processing time\n"
        ));
        output.push_str(&format!("# TYPE {prefix}server_request_seconds gauge\n"));
        for (zone, stats) in server_stats {
            let zone = label::escape(zone);
            for (kind, value) in [
                ("avg", stats.request_times.avg),
                ("min", stats.request_times.min),
                ("max", stats.request_times.max),
            ] {
                output.push_str(&format!(
                    "{prefix}server_request_seconds{{zone=\"{zone}\",type=\"{kind}\"}} {value:.6}\n"
                ));
            }
        }
        output.push('\n');

        // Server-zone request-time distribution (classic histogram).
        self.format_server_request_histogram(&mut output, server_stats);

        output
    }

    /// `nginx_vts_server_request_duration_seconds` classic histogram
    /// (`_bucket{le="..."}`, `_sum`, `_count`).  Mirrors the upstream
    /// histogram layout so dashboards can share bucket definitions,
    /// and enables `histogram_quantile()` for per-vhost p50/p90/p99.
    fn format_server_request_histogram(
        &self,
        output: &mut String,
        server_stats: &HashMap<String, VtsServerStats>,
    ) {
        let prefix = &self.metric_prefix;
        output.push_str(&format!(
            "# HELP {prefix}server_request_duration_seconds Server-zone request processing time distribution\n"
        ));
        output.push_str(&format!(
            "# TYPE {prefix}server_request_duration_seconds histogram\n"
        ));

        for (zone, stats) in server_stats {
            let zone = label::escape(zone);
            for (i, &bound_ms) in RESPONSE_TIME_BUCKET_BOUNDS_MS.iter().enumerate() {
                let bound_s = bound_ms as f64 / 1000.0;
                output.push_str(&format!(
                    "{prefix}server_request_duration_seconds_bucket{{zone=\"{zone}\",le=\"{}\"}} {}\n",
                    format_le_bound(bound_s),
                    stats.request_buckets[i]
                ));
            }
            // +Inf bucket = total request count.
            output.push_str(&format!(
                "{prefix}server_request_duration_seconds_bucket{{zone=\"{zone}\",le=\"+Inf\"}} {}\n",
                stats.requests
            ));
            // `_sum` is the per-zone total seconds spent processing requests,
            // already tracked in `request_times.total` (seconds).
            output.push_str(&format!(
                "{prefix}server_request_duration_seconds_sum{{zone=\"{zone}\"}} {:.6}\n",
                stats.request_times.total
            ));
            output.push_str(&format!(
                "{prefix}server_request_duration_seconds_count{{zone=\"{zone}\"}} {}\n",
                stats.requests
            ));
        }
        output.push('\n');
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{VtsRequestTimes, VtsResponseStats};

    #[test]
    fn format_server_stats_emits_all_families() {
        let mut zones: HashMap<String, VtsServerStats> = HashMap::new();
        zones.insert(
            "example.test".into(),
            VtsServerStats {
                requests: 42,
                bytes_in: 1024,
                bytes_out: 2048,
                responses: VtsResponseStats {
                    status_1xx: 0,
                    status_2xx: 40,
                    status_3xx: 0,
                    status_4xx: 1,
                    status_5xx: 1,
                },
                request_times: VtsRequestTimes {
                    total: 4.2,
                    min: 0.005,
                    max: 0.250,
                    avg: 0.100,
                },
                // 42 samples distributed across buckets — values
                // chosen so the histogram asserts below are crisp:
                // monotone non-decreasing and ≤ 42.
                request_buckets: [10, 20, 30, 35, 40, 41, 42, 42, 42, 42, 42],
            },
        );

        let out = PrometheusFormatter::new().format_server_stats(&zones);
        assert!(out.contains("nginx_vts_server_requests_total{zone=\"example.test\"} 42"));
        assert!(out
            .contains("nginx_vts_server_bytes_total{zone=\"example.test\",direction=\"in\"} 1024"));
        assert!(out.contains(
            "nginx_vts_server_bytes_total{zone=\"example.test\",direction=\"out\"} 2048"
        ));
        assert!(out
            .contains("nginx_vts_server_responses_total{zone=\"example.test\",status=\"2xx\"} 40"));
        assert!(out
            .contains("nginx_vts_server_responses_total{zone=\"example.test\",status=\"4xx\"} 1"));
        assert!(out.contains(
            "nginx_vts_server_request_seconds{zone=\"example.test\",type=\"avg\"} 0.100000"
        ));
        assert!(out.contains(
            "nginx_vts_server_request_seconds{zone=\"example.test\",type=\"min\"} 0.005000"
        ));

        // Histogram: HELP/TYPE headers, a few representative
        // buckets, +Inf, _sum, _count.
        assert!(out.contains(
            "# HELP nginx_vts_server_request_duration_seconds Server-zone request processing time distribution"
        ));
        assert!(out.contains("# TYPE nginx_vts_server_request_duration_seconds histogram"));
        assert!(out.contains(
            "nginx_vts_server_request_duration_seconds_bucket{zone=\"example.test\",le=\"0.005\"} 10"
        ));
        assert!(out.contains(
            "nginx_vts_server_request_duration_seconds_bucket{zone=\"example.test\",le=\"0.1\"} 40"
        ));
        assert!(out.contains(
            "nginx_vts_server_request_duration_seconds_bucket{zone=\"example.test\",le=\"+Inf\"} 42"
        ));
        assert!(out.contains(
            "nginx_vts_server_request_duration_seconds_sum{zone=\"example.test\"} 4.200000"
        ));
        assert!(out
            .contains("nginx_vts_server_request_duration_seconds_count{zone=\"example.test\"} 42"));
    }

    #[test]
    fn a_zone_name_with_a_quote_is_escaped_in_every_family() {
        // `server_name 'a"b';` is a legal configuration line, and one such
        // name unescaped makes the whole response unparseable - not just its
        // own line.
        let mut zones: HashMap<String, VtsServerStats> = HashMap::new();
        zones.insert("a\"b".into(), VtsServerStats::default());

        let out = PrometheusFormatter::new().format_server_stats(&zones);

        for line in out.lines().filter(|l| l.starts_with("nginx_vts_server_")) {
            assert!(
                line.contains("zone=\"a\\\"b\""),
                "unescaped zone in: {line}"
            );
        }
    }
}
