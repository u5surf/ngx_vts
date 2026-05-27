//! `nginx_vts_connections` and `nginx_vts_connections_total` series.

use super::PrometheusFormatter;
use crate::stats::VtsConnectionStats;

impl PrometheusFormatter {
    /// Format connection statistics into Prometheus metrics.
    pub fn format_connection_stats(&self, connections: &VtsConnectionStats) -> String {
        let mut output = String::new();
        let prefix = &self.metric_prefix;

        // Current connection states (gauge).
        output.push_str(&format!(
            "# HELP {prefix}connections Current nginx connections\n"
        ));
        output.push_str(&format!("# TYPE {prefix}connections gauge\n"));
        for (state, value) in [
            ("active", connections.active),
            ("reading", connections.reading),
            ("writing", connections.writing),
            ("waiting", connections.waiting),
        ] {
            output.push_str(&format!(
                "{prefix}connections{{state=\"{state}\"}} {value}\n"
            ));
        }
        output.push('\n');

        // Lifetime totals (counter).
        output.push_str(&format!(
            "# HELP {prefix}connections_total Total nginx connections\n"
        ));
        output.push_str(&format!("# TYPE {prefix}connections_total counter\n"));
        for (state, value) in [
            ("accepted", connections.accepted),
            ("handled", connections.handled),
        ] {
            output.push_str(&format!(
                "{prefix}connections_total{{state=\"{state}\"}} {value}\n"
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
        assert!(out.contains("nginx_vts_connections{state=\"active\"} 7"));
        assert!(out.contains("nginx_vts_connections{state=\"reading\"} 1"));
        assert!(out.contains("nginx_vts_connections{state=\"writing\"} 2"));
        assert!(out.contains("nginx_vts_connections{state=\"waiting\"} 4"));
        assert!(out.contains("nginx_vts_connections_total{state=\"accepted\"} 1000"));
        assert!(out.contains("nginx_vts_connections_total{state=\"handled\"} 999"));
    }
}
