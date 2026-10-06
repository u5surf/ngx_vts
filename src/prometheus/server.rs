//! `nginx_vts_server_*` series, labelled `host` as in the original module.
//!
//! Every zone is followed by the original's aggregate row, `host="*"`,
//! which sums all of them. Dashboards written against the original read
//! the server-wide totals from that row.

use std::collections::HashMap;

use super::{format_le_bound, label, PrometheusFormatter};
use crate::stats::VtsServerStats;
use crate::upstream_stats::RESPONSE_TIME_BUCKET_BOUNDS_MS;

/// The `host` the original module gives the row that sums every zone
/// (`vhost_traffic_status_display_sum_key`'s default).
const SUM_KEY: &str = "*";

impl PrometheusFormatter {
    pub fn format_server_stats(&self, server_stats: &HashMap<String, VtsServerStats>) -> String {
        let prefix = &self.metric_prefix;
        let sum = sum_zones(server_stats);
        let rows: Vec<(std::borrow::Cow<'_, str>, &VtsServerStats)> = server_stats
            .iter()
            .map(|(host, stats)| (label::escape(host), stats))
            .chain(std::iter::once((label::escape(SUM_KEY), &sum)))
            .collect();

        let mut output = format!(
            "# HELP {prefix}server_bytes_total The request/response bytes\n\
             # TYPE {prefix}server_bytes_total counter\n"
        );
        for (host, stats) in &rows {
            output.push_str(&format!(
                "{prefix}server_bytes_total{{host=\"{host}\",direction=\"in\"}} {}\n\
                 {prefix}server_bytes_total{{host=\"{host}\",direction=\"out\"}} {}\n",
                stats.bytes_in, stats.bytes_out
            ));
        }
        output.push('\n');

        output.push_str(&format!(
            "# HELP {prefix}server_requests_total The requests counter\n\
             # TYPE {prefix}server_requests_total counter\n"
        ));
        for (host, stats) in &rows {
            for (code, value) in [
                ("1xx", stats.responses.status_1xx),
                ("2xx", stats.responses.status_2xx),
                ("3xx", stats.responses.status_3xx),
                ("4xx", stats.responses.status_4xx),
                ("5xx", stats.responses.status_5xx),
            ] {
                output.push_str(&format!(
                    "{prefix}server_requests_total{{host=\"{host}\",code=\"{code}\"}} {value}\n"
                ));
            }
        }
        output.push('\n');

        output.push_str(&format!(
            "# HELP {prefix}server_request_seconds_total The request processing time in seconds\n\
             # TYPE {prefix}server_request_seconds_total counter\n"
        ));
        for (host, stats) in &rows {
            output.push_str(&format!(
                "{prefix}server_request_seconds_total{{host=\"{host}\"}} {:.3}\n",
                stats.request_times.total
            ));
        }
        output.push('\n');

        output.push_str(&format!(
            "# HELP {prefix}server_request_seconds The average of request processing times in seconds\n\
             # TYPE {prefix}server_request_seconds gauge\n"
        ));
        for (host, stats) in &rows {
            output.push_str(&format!(
                "{prefix}server_request_seconds{{host=\"{host}\"}} {:.3}\n",
                stats.request_times.avg
            ));
        }
        output.push('\n');

        output.push_str(&format!(
            "# HELP {prefix}server_request_duration_seconds The histogram of request processing time\n\
             # TYPE {prefix}server_request_duration_seconds histogram\n"
        ));
        for (host, stats) in &rows {
            for (i, &bound_ms) in RESPONSE_TIME_BUCKET_BOUNDS_MS.iter().enumerate() {
                output.push_str(&format!(
                    "{prefix}server_request_duration_seconds_bucket{{host=\"{host}\",le=\"{}\"}} {}\n",
                    format_le_bound(bound_ms as f64 / 1000.0),
                    stats.request_buckets[i]
                ));
            }
            output.push_str(&format!(
                "{prefix}server_request_duration_seconds_bucket{{host=\"{host}\",le=\"+Inf\"}} {requests}\n\
                 {prefix}server_request_duration_seconds_sum{{host=\"{host}\"}} {:.3}\n\
                 {prefix}server_request_duration_seconds_count{{host=\"{host}\"}} {requests}\n",
                stats.request_times.total,
                requests = stats.requests
            ));
        }
        output.push('\n');

        output
    }
}

/// The `host="*"` row: every zone's counters added together, with the
/// average taken over the combined requests.
fn sum_zones(server_stats: &HashMap<String, VtsServerStats>) -> VtsServerStats {
    let mut sum = VtsServerStats::default();
    for stats in server_stats.values() {
        sum.requests += stats.requests;
        sum.bytes_in += stats.bytes_in;
        sum.bytes_out += stats.bytes_out;
        sum.responses.status_1xx += stats.responses.status_1xx;
        sum.responses.status_2xx += stats.responses.status_2xx;
        sum.responses.status_3xx += stats.responses.status_3xx;
        sum.responses.status_4xx += stats.responses.status_4xx;
        sum.responses.status_5xx += stats.responses.status_5xx;
        sum.request_times.total += stats.request_times.total;
        for (total, bucket) in sum.request_buckets.iter_mut().zip(stats.request_buckets) {
            *total += bucket;
        }
    }
    if sum.requests > 0 {
        sum.request_times.avg = sum.request_times.total / sum.requests as f64;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stats::{VtsRequestTimes, VtsResponseStats};

    fn zone(requests: u64, total: f64) -> VtsServerStats {
        VtsServerStats {
            requests,
            bytes_in: 1024,
            bytes_out: 2048,
            responses: VtsResponseStats {
                status_1xx: 0,
                status_2xx: requests - 2,
                status_3xx: 0,
                status_4xx: 1,
                status_5xx: 1,
            },
            request_times: VtsRequestTimes {
                total,
                min: 0.005,
                max: 0.250,
                avg: total / requests as f64,
            },
            request_buckets: [10, 20, 30, 35, 40, 41, 42, 42, 42, 42, 42],
        }
    }

    #[test]
    fn format_server_stats_emits_all_families() {
        let zones = HashMap::from([("example.test".to_string(), zone(42, 4.2))]);

        let out = PrometheusFormatter::new().format_server_stats(&zones);
        assert!(out
            .contains("nginx_vts_server_bytes_total{host=\"example.test\",direction=\"in\"} 1024"));
        assert!(out.contains(
            "nginx_vts_server_bytes_total{host=\"example.test\",direction=\"out\"} 2048"
        ));
        assert!(
            out.contains("nginx_vts_server_requests_total{host=\"example.test\",code=\"2xx\"} 40")
        );
        assert!(
            out.contains("nginx_vts_server_requests_total{host=\"example.test\",code=\"4xx\"} 1")
        );
        assert!(out.contains("nginx_vts_server_request_seconds_total{host=\"example.test\"} 4.200"));
        assert!(out.contains("nginx_vts_server_request_seconds{host=\"example.test\"} 0.100"));

        assert!(out.contains("# TYPE nginx_vts_server_requests_total counter"));
        assert!(out.contains("# TYPE nginx_vts_server_request_seconds_total counter"));
        assert!(out.contains("# TYPE nginx_vts_server_request_seconds gauge"));
        assert!(out.contains("# TYPE nginx_vts_server_request_duration_seconds histogram"));
        assert!(out.contains(
            "nginx_vts_server_request_duration_seconds_bucket{host=\"example.test\",le=\"0.005\"} 10"
        ));
        assert!(out.contains(
            "nginx_vts_server_request_duration_seconds_bucket{host=\"example.test\",le=\"+Inf\"} 42"
        ));
        assert!(out.contains(
            "nginx_vts_server_request_duration_seconds_sum{host=\"example.test\"} 4.200"
        ));
        assert!(out
            .contains("nginx_vts_server_request_duration_seconds_count{host=\"example.test\"} 42"));
    }

    #[test]
    fn there_is_no_unlabelled_request_total() {
        let zones = HashMap::from([("example.test".to_string(), zone(42, 4.2))]);

        let out = PrometheusFormatter::new().format_server_stats(&zones);
        assert!(!out.contains("nginx_vts_server_requests_total{host=\"example.test\"}"));
    }

    #[test]
    fn the_star_row_sums_every_zone() {
        let zones = HashMap::from([
            ("a.test".to_string(), zone(10, 1.0)),
            ("b.test".to_string(), zone(30, 5.0)),
        ]);

        let out = PrometheusFormatter::new().format_server_stats(&zones);
        assert!(out.contains("nginx_vts_server_bytes_total{host=\"*\",direction=\"in\"} 2048"));
        assert!(out.contains("nginx_vts_server_requests_total{host=\"*\",code=\"2xx\"} 36"));
        assert!(out.contains("nginx_vts_server_requests_total{host=\"*\",code=\"5xx\"} 2"));
        assert!(out.contains("nginx_vts_server_request_seconds_total{host=\"*\"} 6.000"));
        assert!(out.contains("nginx_vts_server_request_seconds{host=\"*\"} 0.150"));
        assert!(out.contains(
            "nginx_vts_server_request_duration_seconds_bucket{host=\"*\",le=\"0.005\"} 20"
        ));
        assert!(out.contains("nginx_vts_server_request_duration_seconds_count{host=\"*\"} 40"));
    }

    #[test]
    fn the_star_row_is_there_before_any_traffic() {
        let out = PrometheusFormatter::new().format_server_stats(&HashMap::new());
        assert!(out.contains("nginx_vts_server_requests_total{host=\"*\",code=\"2xx\"} 0"));
        assert!(out.contains("nginx_vts_server_request_seconds{host=\"*\"} 0.000"));
    }

    #[test]
    fn a_host_with_a_quote_is_escaped_in_every_family() {
        let zones = HashMap::from([("a\"b".to_string(), VtsServerStats::default())]);

        let out = PrometheusFormatter::new().format_server_stats(&zones);

        for line in out
            .lines()
            .filter(|l| l.starts_with("nginx_vts_server_") && !l.contains("host=\"*\""))
        {
            assert!(
                line.contains("host=\"a\\\"b\""),
                "unescaped host in: {line}"
            );
        }
    }
}
