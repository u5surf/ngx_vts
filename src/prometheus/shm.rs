//! `nginx_vts_main_shm_usage_bytes` and `nginx_vts_main_shm_usage_nodes`.
//!
//! Ported from the shared-zone half of the original module's main format
//! string, which nginx-module-vts covers in t/033.shm_free_size.t.

use super::PrometheusFormatter;
use crate::shm::ShmInfo;

impl PrometheusFormatter {
    /// Format the shared zone's usage.
    ///
    /// `used_size` is not reported. The original sums the sizes of its nodes,
    /// but the slab hands out a whole page or slot for each of them, so that
    /// figure can sit well below the maximum while the zone is already
    /// refusing inserts. `free_size` is what says whether another node fits.
    pub fn format_shm_info(&self, info: &ShmInfo) -> String {
        let prefix = &self.metric_prefix;
        let mut output = String::new();

        output.push_str(&format!(
            "# HELP {prefix}main_shm_usage_bytes Shared memory zone usage\n"
        ));
        output.push_str(&format!("# TYPE {prefix}main_shm_usage_bytes gauge\n"));
        for (shared, value) in [("max_size", info.max_size), ("free_size", info.free_size)] {
            output.push_str(&format!(
                "{prefix}main_shm_usage_bytes{{shared=\"{shared}\"}} {value}\n"
            ));
        }
        output.push('\n');

        output.push_str(&format!(
            "# HELP {prefix}main_shm_usage_nodes Entries held in the shared zone\n"
        ));
        output.push_str(&format!("# TYPE {prefix}main_shm_usage_nodes gauge\n"));
        output.push_str(&format!(
            "{prefix}main_shm_usage_nodes {}\n",
            info.used_node
        ));
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
    fn reports_the_configured_maximum_and_what_is_left() {
        let out = formatter().format_shm_info(&ShmInfo {
            max_size: 1_048_576,
            free_size: 942_080,
            used_node: 3,
        });

        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"max_size\"} 1048576"));
        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"free_size\"} 942080"));
        assert!(out.contains("nginx_vts_main_shm_usage_nodes 3"));
    }

    #[test]
    fn declares_both_families() {
        let out = formatter().format_shm_info(&ShmInfo::default());

        assert!(out.contains("# TYPE nginx_vts_main_shm_usage_bytes gauge"));
        assert!(out.contains("# TYPE nginx_vts_main_shm_usage_nodes gauge"));
    }

    #[test]
    fn an_empty_zone_reports_zeroes_rather_than_nothing() {
        let out = formatter().format_shm_info(&ShmInfo::default());

        assert!(out.contains("nginx_vts_main_shm_usage_bytes{shared=\"free_size\"} 0"));
        assert!(out.contains("nginx_vts_main_shm_usage_nodes 0"));
    }
}
