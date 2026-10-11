//! `nginx_vts_server_*` series, labelled `host` as in the original module,
//! and `nginx_vts_status_code_requests_total`, which breaks the same zones
//! down by status code when `vts_measure_status_codes` asks for it.
//!
//! Every zone is followed by the original's aggregate row, `host="*"`,
//! which sums all of them. Dashboards written against the original read
//! the server-wide totals from that row.

use std::collections::{BTreeMap, HashMap};

use super::{format_le_bound, label, PrometheusFormatter};
use crate::stats::VtsServerStats;
use crate::status_codes::{MeasureStatusCodes, StatusCodeSlot};
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

impl PrometheusFormatter {
    /// `nginx_vts_status_code_requests_total`, as the original prints it:
    /// every zone gets an `other` row and, for a list of codes, a row per
    /// listed code even while it reads zero. With `all` only the codes that
    /// occurred are printed, since the other few hundred would all be zero.
    /// Nothing at all is printed when the directive is absent.
    pub fn format_status_code_stats(
        &self,
        server_stats: &HashMap<String, VtsServerStats>,
        measure: &MeasureStatusCodes,
    ) -> String {
        let listed: &[u16] = match measure {
            MeasureStatusCodes::Off => return String::new(),
            MeasureStatusCodes::All => &[],
            MeasureStatusCodes::Listed(codes) => codes,
        };

        let prefix = &self.metric_prefix;
        let mut sum: BTreeMap<StatusCodeSlot, u64> = BTreeMap::new();
        for stats in server_stats.values() {
            for (&slot, &count) in &stats.status_codes {
                *sum.entry(slot).or_insert(0) += count;
            }
        }
        let rows = server_stats
            .iter()
            .map(|(host, stats)| (label::escape(host), &stats.status_codes))
            .chain(std::iter::once((label::escape(SUM_KEY), &sum)));

        let mut output = format!(
            "# HELP {prefix}status_code_requests_total The requests counter by status code
             # TYPE {prefix}status_code_requests_total counter
"
        );
        for (host, codes) in rows {
            let mut slots: BTreeMap<StatusCodeSlot, u64> = listed
                .iter()
                .map(|&code| (StatusCodeSlot::Code(code), 0))
                .chain(std::iter::once((StatusCodeSlot::Other, 0)))
                .collect();
            slots.extend(codes.iter().map(|(&slot, &count)| (slot, count)));
            for (slot, count) in slots {
                output.push_str(&format!(
                    "{prefix}status_code_requests_total{{host=\"{host}\",code=\"{}\"}} {count}\n",
                    slot.label()
                ));
            }
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
            status_codes: BTreeMap::new(),
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

    fn with_codes(codes: &[(StatusCodeSlot, u64)]) -> VtsServerStats {
        VtsServerStats {
            status_codes: codes.iter().copied().collect(),
            ..VtsServerStats::default()
        }
    }

    #[test]
    fn status_codes_are_not_printed_unless_measured() {
        let zones = HashMap::from([(
            "example.test".to_string(),
            with_codes(&[(StatusCodeSlot::Code(200), 1)]),
        )]);

        let out =
            PrometheusFormatter::new().format_status_code_stats(&zones, &MeasureStatusCodes::Off);
        assert_eq!(out, "");
    }

    #[test]
    fn listed_codes_are_printed_even_at_zero() {
        let zones = HashMap::from([(
            "example.test".to_string(),
            with_codes(&[(StatusCodeSlot::Code(404), 3), (StatusCodeSlot::Other, 2)]),
        )]);
        let measure = MeasureStatusCodes::Listed(vec![200, 404]);

        let out = PrometheusFormatter::new().format_status_code_stats(&zones, &measure);
        assert!(out.contains("# TYPE nginx_vts_status_code_requests_total counter\n"));
        for line in [
            "nginx_vts_status_code_requests_total{host=\"example.test\",code=\"200\"} 0\n",
            "nginx_vts_status_code_requests_total{host=\"example.test\",code=\"404\"} 3\n",
            "nginx_vts_status_code_requests_total{host=\"example.test\",code=\"other\"} 2\n",
        ] {
            assert!(out.contains(line), "missing {line:?} in:\n{out}");
        }
    }

    #[test]
    fn all_prints_only_the_codes_that_occurred() {
        let zones = HashMap::from([(
            "example.test".to_string(),
            with_codes(&[(StatusCodeSlot::Code(418), 1)]),
        )]);

        let out =
            PrometheusFormatter::new().format_status_code_stats(&zones, &MeasureStatusCodes::All);
        assert!(out.contains("{host=\"example.test\",code=\"418\"} 1\n"));
        assert!(out.contains("{host=\"example.test\",code=\"other\"} 0\n"));
        assert_eq!(out.matches("host=\"example.test\"").count(), 2);
    }

    #[test]
    fn the_star_row_sums_the_codes_of_every_zone() {
        let zones = HashMap::from([
            (
                "a.test".to_string(),
                with_codes(&[(StatusCodeSlot::Code(200), 2)]),
            ),
            (
                "b.test".to_string(),
                with_codes(&[
                    (StatusCodeSlot::Code(200), 3),
                    (StatusCodeSlot::Code(503), 1),
                ]),
            ),
        ]);

        let out =
            PrometheusFormatter::new().format_status_code_stats(&zones, &MeasureStatusCodes::All);
        assert!(out.contains("{host=\"*\",code=\"200\"} 5\n"));
        assert!(out.contains("{host=\"*\",code=\"503\"} 1\n"));
        assert!(out.contains("{host=\"*\",code=\"other\"} 0\n"));
    }

    #[test]
    fn the_star_row_is_there_before_any_traffic_when_measured() {
        let measure = MeasureStatusCodes::Listed(vec![500]);
        let out = PrometheusFormatter::new().format_status_code_stats(&HashMap::new(), &measure);
        assert!(out.contains("nginx_vts_status_code_requests_total{host=\"*\",code=\"500\"} 0\n"));
        assert!(out.contains("nginx_vts_status_code_requests_total{host=\"*\",code=\"other\"} 0\n"));
    }

    #[test]
    fn a_host_with_a_quote_is_escaped_in_the_status_code_rows() {
        let zones = HashMap::from([("a\"b".to_string(), VtsServerStats::default())]);

        let out =
            PrometheusFormatter::new().format_status_code_stats(&zones, &MeasureStatusCodes::All);
        assert!(out.contains("{host=\"a\\\"b\",code=\"other\"} 0\n"));
    }
}
