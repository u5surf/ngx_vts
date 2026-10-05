//! `nginx_vts_main_shm_usage_bytes`, with the original module's four
//! `shared` labels.
//!
//! Ported from the shared-zone half of the original module's main format
//! string, which nginx-module-vts covers in t/033.shm_free_size.t.

use super::PrometheusFormatter;
use crate::shm::ShmInfo;

impl PrometheusFormatter {
    /// Format the shared zone's usage.
    ///
    /// `used_size` is the configured size less what the slab has left,
    /// not the original's sum of node sizes. The slab hands out a whole
    /// page or slot for each node, so that sum can sit well below the
    /// maximum while the zone is already refusing inserts; what the slab
    /// has spent is what tells an operator the zone is full. `used_node`
    /// is a count, not bytes, and sits under this family only because the
    /// original put it there.
    pub fn format_shm_info(&self, info: &ShmInfo) -> String {
        let prefix = &self.metric_prefix;
        let mut output = format!(
            "# HELP {prefix}main_shm_usage_bytes Shared memory zone usage\n\
             # TYPE {prefix}main_shm_usage_bytes gauge\n"
        );
        for (shared, value) in [
            ("max_size", info.max_size),
            ("used_size", info.max_size.saturating_sub(info.free_size)),
            ("used_node", info.used_node),
            ("free_size", info.free_size),
        ] {
            output.push_str(&format!(
                "{prefix}main_shm_usage_bytes{{shared=\"{shared}\"}} {value}\n"
            ));
        }
        output.push('\n');

        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn formatter() -> PrometheusFormatter {
        PrometheusFormatter::new()
    }

    #[test]
    fn reports_all_four_shared_labels() {
        let out = formatter().format_shm_info(&ShmInfo {
            max_size: 1_048_576,
            free_size: 942_080,
            used_node: 3,
        });

        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"max_size\"} 1048576"));
        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"used_size\"} 106496"));
        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"used_node\"} 3"));
        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"free_size\"} 942080"));
    }

    #[test]
    fn declares_a_gauge() {
        let out = formatter().format_shm_info(&ShmInfo::default());

        assert!(out.contains("# TYPE nginx_vts_main_shm_usage_bytes gauge"));
    }

    #[test]
    fn an_empty_zone_reports_zeroes_rather_than_nothing() {
        let out = formatter().format_shm_info(&ShmInfo::default());

        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"used_size\"} 0"));
        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"free_size\"} 0"));
        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"used_node\"} 0"));
    }
}
