//! `nginx_vts_cache_*` series, labelled `cache_zone` as in the original
//! module: size gauges and request counters by cache status.

use std::collections::HashMap;

use super::{label, PrometheusFormatter};
use crate::cache_stats::CacheZoneStats;

impl PrometheusFormatter {
    /// Format cache statistics to Prometheus metrics.
    ///
    /// Renders nothing when no cache zone has been used, as the original
    /// leaves the whole section out.
    pub fn format_cache_stats(&self, cache_zones: &HashMap<String, CacheZoneStats>) -> String {
        let mut output = String::new();
        if cache_zones.is_empty() {
            return output;
        }
        let prefix = &self.metric_prefix;

        output.push_str(&format!(
            "# HELP {prefix}cache_requests_total The cache requests counter\n\
             # TYPE {prefix}cache_requests_total counter\n"
        ));
        for zone_stats in cache_zones.values() {
            let zone = label::escape(&zone_stats.name);
            for (status, value) in [
                ("hit", zone_stats.cache.hit),
                ("miss", zone_stats.cache.miss),
                ("bypass", zone_stats.cache.bypass),
                ("expired", zone_stats.cache.expired),
                ("stale", zone_stats.cache.stale),
                ("updating", zone_stats.cache.updating),
                ("revalidated", zone_stats.cache.revalidated),
                ("scarce", zone_stats.cache.scarce),
            ] {
                output.push_str(&format!(
                    "{prefix}cache_requests_total{{cache_zone=\"{zone}\",status=\"{status}\"}} {value}\n"
                ));
            }
        }
        output.push('\n');

        output.push_str(&format!(
            "# HELP {prefix}cache_usage_bytes The cache zones info\n\
             # TYPE {prefix}cache_usage_bytes gauge\n"
        ));
        for zone_stats in cache_zones.values() {
            let zone = label::escape(&zone_stats.name);
            output.push_str(&format!(
                "{prefix}cache_usage_bytes{{cache_zone=\"{zone}\",cache_size=\"max\"}} {}\n\
                 {prefix}cache_usage_bytes{{cache_zone=\"{zone}\",cache_size=\"used\"}} {}\n",
                zone_stats.size.max_size, zone_stats.size.used_size
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
    fn empty_cache_zones_render_nothing() {
        let empty: HashMap<String, CacheZoneStats> = HashMap::new();
        assert!(PrometheusFormatter::new()
            .format_cache_stats(&empty)
            .is_empty());
    }

    #[test]
    fn populated_cache_zone_renders_both_families() {
        let mut zones = HashMap::new();
        let mut zone = CacheZoneStats::new("test_cache");
        zone.cache.hit = 7;
        zone.cache.miss = 3;
        zone.size.max_size = 1_048_576;
        zone.size.used_size = 524_288;
        zones.insert("test_cache".into(), zone);

        let out = PrometheusFormatter::new().format_cache_stats(&zones);
        assert!(out.contains("# TYPE nginx_vts_cache_usage_bytes gauge"));
        assert!(out.contains("# TYPE nginx_vts_cache_requests_total counter"));
        assert!(out.contains(
            "nginx_vts_cache_requests_total{cache_zone=\"test_cache\",status=\"hit\"} 7"
        ));
        assert!(out.contains(
            "nginx_vts_cache_requests_total{cache_zone=\"test_cache\",status=\"miss\"} 3"
        ));
        assert!(out.contains(
            "nginx_vts_cache_usage_bytes{cache_zone=\"test_cache\",cache_size=\"max\"} 1048576"
        ));
        assert!(out.contains(
            "nginx_vts_cache_usage_bytes{cache_zone=\"test_cache\",cache_size=\"used\"} 524288"
        ));
    }
}
