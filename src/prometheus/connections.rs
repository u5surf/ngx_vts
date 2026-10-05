//! `nginx_vts_main_connections`, ported from the original module's main
//! block.  The original declares every state a gauge, `accepted` and
//! `handled` included, so this does too: a query that
//! wraps them in `rate()` works either way, and one written against the
//! original keeps its series.

use super::PrometheusFormatter;
use crate::stats::VtsConnectionStats;

impl PrometheusFormatter {
    /// Format connection statistics into Prometheus metrics.
    pub fn format_connection_stats(&self, connections: &VtsConnectionStats) -> String {
        let prefix = &self.metric_prefix;
        let mut output = format!(
            "# HELP {prefix}main_connections Nginx connections\n\
             # TYPE {prefix}main_connections gauge\n"
        );
        for (status, value) in [
            ("accepted", connections.accepted),
            ("active", connections.active),
            ("handled", connections.handled),
            ("reading", connections.reading),
            ("waiting", connections.waiting),
            ("writing", connections.writing),
        ] {
            output.push_str(&format!(
                "{prefix}main_connections{{status=\"{status}\"}} {value}\n"
            ));
        }
        output.push('\n');

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_connection_stats_emits_all_six_states() {
        let stats = VtsConnectionStats {
            active: 7,
            reading: 1,
            writing: 2,
            waiting: 4,
            accepted: 1000,
            handled: 999,
        };
        let out = PrometheusFormatter::new().format_connection_stats(&stats);
        assert!(out.contains("# TYPE nginx_vts_main_connections gauge"));
        assert!(out.contains("nginx_vts_main_connections{status=\"accepted\"} 1000"));
        assert!(out.contains("nginx_vts_main_connections{status=\"active\"} 7"));
        assert!(out.contains("nginx_vts_main_connections{status=\"handled\"} 999"));
        assert!(out.contains("nginx_vts_main_connections{status=\"reading\"} 1"));
        assert!(out.contains("nginx_vts_main_connections{status=\"waiting\"} 4"));
        assert!(out.contains("nginx_vts_main_connections{status=\"writing\"} 2"));
    }
}
