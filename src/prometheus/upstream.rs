//! `nginx_vts_upstream_*` series, labelled `upstream` and `backend` as in
//! the original module: bytes, status-class request counters, request and
//! response time totals and averages, and the `request_duration_seconds`
//! and `response_duration_seconds` classic histograms (compatible with
//! `histogram_quantile()` for p50/p90/p99 panels).
//!
//! `upstream_server_up` has no counterpart in the original; it comes from
//! a walk of the group rather than from the counters.

use std::collections::HashMap;

use super::{format_le_bound, label, PrometheusFormatter};
use crate::upstream_stats::{
    UpstreamServerStats, UpstreamZone, RESPONSE_TIME_BUCKET_BOUNDS_MS, RESPONSE_TIME_BUCKET_COUNT,
};

impl PrometheusFormatter {
    /// Format upstream statistics into Prometheus metrics.
    ///
    /// Renders nothing when no upstream has been used, as the original
    /// leaves the whole section out.
    pub fn format_upstream_stats(
        &self,
        upstream_zones: &HashMap<String, UpstreamZone>,
        peer_states: &HashMap<(String, String), crate::peers::PeerState>,
    ) -> String {
        let mut output = String::new();
        if upstream_zones.is_empty() {
            return output;
        }
        let prefix = &self.metric_prefix;
        let rows: Vec<(String, &UpstreamServerStats)> = upstream_zones
            .iter()
            .flat_map(|(upstream, zone)| {
                let upstream = label::escape(upstream).into_owned();
                zone.servers.iter().map(move |(backend, stats)| {
                    (
                        format!(
                            "upstream=\"{upstream}\",backend=\"{}\"",
                            label::escape(backend)
                        ),
                        stats,
                    )
                })
            })
            .collect();

        output.push_str(&format!(
            "# HELP {prefix}upstream_bytes_total The request/response bytes\n\
             # TYPE {prefix}upstream_bytes_total counter\n"
        ));
        for (labels, stats) in &rows {
            output.push_str(&format!(
                "{prefix}upstream_bytes_total{{{labels},direction=\"in\"}} {}\n\
                 {prefix}upstream_bytes_total{{{labels},direction=\"out\"}} {}\n",
                stats.in_bytes, stats.out_bytes
            ));
        }
        output.push('\n');

        output.push_str(&format!(
            "# HELP {prefix}upstream_requests_total The upstream requests counter\n\
             # TYPE {prefix}upstream_requests_total counter\n"
        ));
        for (labels, stats) in &rows {
            for (code, value) in [
                ("1xx", stats.responses.status_1xx),
                ("2xx", stats.responses.status_2xx),
                ("3xx", stats.responses.status_3xx),
                ("4xx", stats.responses.status_4xx),
                ("5xx", stats.responses.status_5xx),
            ] {
                output.push_str(&format!(
                    "{prefix}upstream_requests_total{{{labels},code=\"{code}\"}} {value}\n"
                ));
            }
        }
        output.push('\n');

        // Times are kept in milliseconds; the original reports seconds.
        for (name, help, kind, value) in [
            (
                "request_seconds_total",
                "The request Processing time including upstream in seconds",
                "counter",
                (|s: &UpstreamServerStats| s.request_time_total as f64)
                    as fn(&UpstreamServerStats) -> f64,
            ),
            (
                "request_seconds",
                "The average of request processing times including upstream in seconds",
                "gauge",
                UpstreamServerStats::avg_request_time,
            ),
            (
                "response_seconds_total",
                "The only upstream response processing time in seconds",
                "counter",
                |s: &UpstreamServerStats| s.response_time_total as f64,
            ),
            (
                "response_seconds",
                "The average of only upstream response processing times in seconds",
                "gauge",
                UpstreamServerStats::avg_response_time,
            ),
        ] {
            output.push_str(&format!(
                "# HELP {prefix}upstream_{name} {help}\n\
                 # TYPE {prefix}upstream_{name} {kind}\n"
            ));
            for (labels, stats) in &rows {
                output.push_str(&format!(
                    "{prefix}upstream_{name}{{{labels}}} {:.3}\n",
                    value(stats) / 1000.0
                ));
            }
            output.push('\n');
        }

        self.format_upstream_histogram(
            &mut output,
            &rows,
            "request_duration_seconds",
            "The histogram of request processing time including upstream",
            |s| (&s.request_buckets, s.request_time_total, s.request_counter),
        );
        self.format_upstream_histogram(
            &mut output,
            &rows,
            "response_duration_seconds",
            "The histogram of only upstream response processing time",
            |s| {
                (
                    &s.response_buckets,
                    s.response_time_total,
                    s.response_time_counter,
                )
            },
        );

        // nginx_vts_upstream_server_up
        //
        // Whether a peer is in rotation is a property of the group, not of
        // anything a request did, so it comes from `peer_states` - a walk of
        // `uscf->peer.data` - rather than from the counters. A peer the walk
        // did not see gets no series: the counters can outlive the group, and
        // saying nothing is better than saying "up" about an address that is
        // no longer configured.
        output.push_str(&format!(
            "# HELP {prefix}upstream_server_up Upstream server status (1=up, 0=down)\n\
             # TYPE {prefix}upstream_server_up gauge\n"
        ));
        for ((upstream, backend), peer) in peer_states {
            let upstream = label::escape(upstream);
            let backend = label::escape(backend);
            let server_up = if peer.down { 0 } else { 1 };
            output.push_str(&format!(
                "{prefix}upstream_server_up{{upstream=\"{upstream}\",backend=\"{backend}\"}} {server_up}\n"
            ));
        }
        output.push('\n');

        output
    }

    /// One `nginx_vts_upstream_<name>` classic histogram
    /// (`_bucket{le="..."}`, `_sum`, `_count`).  `sample` picks the
    /// buckets, the total in milliseconds and the sample count out of a
    /// row; the count is also the `+Inf` bucket.
    fn format_upstream_histogram(
        &self,
        output: &mut String,
        rows: &[(String, &UpstreamServerStats)],
        name: &str,
        help: &str,
        sample: impl Fn(&UpstreamServerStats) -> (&[u64; RESPONSE_TIME_BUCKET_COUNT], u64, u64),
    ) {
        let prefix = &self.metric_prefix;
        output.push_str(&format!(
            "# HELP {prefix}upstream_{name} {help}\n\
             # TYPE {prefix}upstream_{name} histogram\n"
        ));
        for (labels, stats) in rows {
            let (buckets, total_ms, count) = sample(stats);
            for (i, &bound_ms) in RESPONSE_TIME_BUCKET_BOUNDS_MS.iter().enumerate() {
                output.push_str(&format!(
                    "{prefix}upstream_{name}_bucket{{{labels},le=\"{}\"}} {}\n",
                    format_le_bound(bound_ms as f64 / 1000.0),
                    buckets[i]
                ));
            }
            output.push_str(&format!(
                "{prefix}upstream_{name}_bucket{{{labels},le=\"+Inf\"}} {count}\n\
                 {prefix}upstream_{name}_sum{{{labels}}} {:.3}\n\
                 {prefix}upstream_{name}_count{{{labels}}} {count}\n",
                total_ms as f64 / 1000.0
            ));
        }
        output.push('\n');
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
        server1.request_buckets = [5, 15, 30, 50, 70, 90, 97, 99, 100, 100, 100];
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
    fn empty_upstream_zones_render_to_empty_string() {
        let f = PrometheusFormatter::new();
        let empty: HashMap<String, UpstreamZone> = HashMap::new();
        assert!(f
            .format_upstream_stats(&empty, &Default::default())
            .is_empty());
    }

    #[test]
    fn upstream_stats_render_all_families() {
        let mut zones = HashMap::new();
        zones.insert("test_backend".to_string(), create_test_upstream_zone());

        // server_up comes from the group walk rather than the counters, so the
        // test has to say what the group holds. The second peer is down.
        let mut peer_states = HashMap::new();
        for (addr, down) in [("10.0.0.1:80", false), ("10.0.0.2:80", true)] {
            peer_states.insert(
                ("test_backend".to_string(), addr.to_string()),
                crate::peers::PeerState {
                    down,
                    backup: false,
                    weight: 1,
                    max_fails: 1,
                    fails: if down { 1 } else { 0 },
                },
            );
        }

        let out = PrometheusFormatter::new().format_upstream_stats(&zones, &peer_states);

        // Counters / bytes / times / server_up.
        assert!(out.contains("# TYPE nginx_vts_upstream_requests_total counter"));
        assert!(out.contains("nginx_vts_upstream_requests_total{upstream=\"test_backend\",backend=\"10.0.0.1:80\",code=\"2xx\"} 95"));
        assert!(out.contains("nginx_vts_upstream_requests_total{upstream=\"test_backend\",backend=\"10.0.0.1:80\",code=\"4xx\"} 3"));
        assert!(out.contains("nginx_vts_upstream_requests_total{upstream=\"test_backend\",backend=\"10.0.0.1:80\",code=\"5xx\"} 2"));
        assert!(!out.contains(
            "nginx_vts_upstream_requests_total{upstream=\"test_backend\",backend=\"10.0.0.1:80\"}"
        ));
        assert!(out.contains("nginx_vts_upstream_bytes_total{upstream=\"test_backend\",backend=\"10.0.0.1:80\",direction=\"in\"} 50000"));
        assert!(out.contains("nginx_vts_upstream_bytes_total{upstream=\"test_backend\",backend=\"10.0.0.1:80\",direction=\"out\"} 25000"));
        assert!(out.contains(
            "nginx_vts_upstream_server_up{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 1"
        ));
        assert!(out.contains(
            "nginx_vts_upstream_server_up{upstream=\"test_backend\",backend=\"10.0.0.2:80\"} 0"
        ));
        assert!(out.contains("nginx_vts_upstream_request_seconds_total{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 5.000"));
        assert!(out.contains("nginx_vts_upstream_request_seconds{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 0.050"));
        assert!(out.contains("nginx_vts_upstream_response_seconds_total{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 2.500"));
        assert!(out.contains("nginx_vts_upstream_response_seconds{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 0.025"));
        assert!(out.contains("# TYPE nginx_vts_upstream_request_seconds_total counter"));
        assert!(out.contains("# TYPE nginx_vts_upstream_request_seconds gauge"));
        assert!(out.contains("# TYPE nginx_vts_upstream_response_seconds_total counter"));
        assert!(out.contains("# TYPE nginx_vts_upstream_response_seconds gauge"));

        // Histograms, request before response as in the original.
        assert!(out.contains("# TYPE nginx_vts_upstream_request_duration_seconds histogram"));
        assert!(out.contains("nginx_vts_upstream_request_duration_seconds_bucket{upstream=\"test_backend\",backend=\"10.0.0.1:80\",le=\"0.005\"} 5"));
        assert!(out.contains("nginx_vts_upstream_request_duration_seconds_bucket{upstream=\"test_backend\",backend=\"10.0.0.1:80\",le=\"0.1\"} 70"));
        assert!(out.contains("nginx_vts_upstream_request_duration_seconds_bucket{upstream=\"test_backend\",backend=\"10.0.0.1:80\",le=\"+Inf\"} 100"));
        assert!(out.contains("nginx_vts_upstream_request_duration_seconds_sum{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 5.000"));
        assert!(out.contains("nginx_vts_upstream_request_duration_seconds_count{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 100"));
        assert!(
            out.find("# TYPE nginx_vts_upstream_request_duration_seconds histogram")
                < out.find("# TYPE nginx_vts_upstream_response_duration_seconds histogram")
        );
        assert!(out.contains("# TYPE nginx_vts_upstream_response_duration_seconds histogram"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_bucket{upstream=\"test_backend\",backend=\"10.0.0.1:80\",le=\"0.005\"} 10"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_bucket{upstream=\"test_backend\",backend=\"10.0.0.1:80\",le=\"0.1\"} 80"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_bucket{upstream=\"test_backend\",backend=\"10.0.0.1:80\",le=\"1\"} 99"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_bucket{upstream=\"test_backend\",backend=\"10.0.0.1:80\",le=\"+Inf\"} 100"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_sum{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 2.500"));
        assert!(out.contains("nginx_vts_upstream_response_duration_seconds_count{upstream=\"test_backend\",backend=\"10.0.0.1:80\"} 100"));
    }

    #[test]
    fn custom_prefix_replaces_default_throughout() {
        let f = PrometheusFormatter::with_prefix("custom_vts_");
        let mut zones = HashMap::new();
        zones.insert("test_backend".to_string(), create_test_upstream_zone());
        let out = f.format_upstream_stats(&zones, &Default::default());
        assert!(out.contains("# TYPE custom_vts_upstream_requests_total counter"));
        assert!(out.contains("custom_vts_upstream_requests_total{upstream=\"test_backend\""));
        assert!(!out.contains("nginx_vts_"));
    }

    #[test]
    fn a_peer_address_with_a_quote_is_escaped_everywhere_it_appears() {
        // A unix socket path is a peer address, and a path may contain a
        // quote or a backslash. Both the counters and server_up carry it.
        let mut zone = create_test_upstream_zone();
        let stats = zone.servers.values().next().unwrap().clone();
        zone.servers.clear();
        zone.servers
            .insert("unix:/tmp/a\"b.sock".to_string(), stats);

        let mut zones = HashMap::new();
        zones.insert("back\\end".to_string(), zone);

        let mut peer_states = HashMap::new();
        peer_states.insert(
            ("back\\end".to_string(), "unix:/tmp/a\"b.sock".to_string()),
            crate::peers::PeerState {
                down: false,
                backup: false,
                weight: 1,
                max_fails: 1,
                fails: 0,
            },
        );

        let out = PrometheusFormatter::new().format_upstream_stats(&zones, &peer_states);

        let series: Vec<&str> = out
            .lines()
            .filter(|l| l.starts_with("nginx_vts_upstream_"))
            .collect();
        assert!(!series.is_empty());
        for line in series {
            assert!(line.contains(r#"upstream="back\\end""#), "upstream: {line}");
            assert!(
                line.contains(r#"backend="unix:/tmp/a\"b.sock""#),
                "backend: {line}"
            );
        }
        // And server_up, which comes from the group walk rather than the
        // counters, went through the same escaping.
        assert!(out.contains(
            r#"nginx_vts_upstream_server_up{upstream="back\\end",backend="unix:/tmp/a\"b.sock"} 1"#
        ));
    }
}
