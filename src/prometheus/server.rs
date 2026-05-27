//! `nginx_vts_server_*` series (requests / bytes / responses / request_seconds).

use std::collections::HashMap;

use super::PrometheusFormatter;
use crate::stats::VtsServerStats;

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

        output
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
    }
}
