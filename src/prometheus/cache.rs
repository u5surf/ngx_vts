//! `nginx_vts_cache_*` series: request counters, size gauges, hit ratio.

use std::collections::HashMap;

use super::PrometheusFormatter;
use crate::cache_stats::CacheZoneStats;

impl PrometheusFormatter {
    /// Format cache statistics to Prometheus metrics.
    pub fn format_cache_stats(&self, cache_zones: &HashMap<String, CacheZoneStats>) -> String {
        let mut output = String::new();

        if cache_zones.is_empty() {
            // Always emit the HELP/TYPE headers so scrapers can see
            // the metric exists even before any cache traffic.
            output
                .push_str("# HELP nginx_vts_cache_requests_total Total number of cache requests\n");
            output.push_str("# TYPE nginx_vts_cache_requests_total counter\n");
            output.push_str("# HELP nginx_vts_cache_size_bytes Cache size statistics in bytes\n");
            output.push_str("# TYPE nginx_vts_cache_size_bytes gauge\n\n");
            return output;
        }

        let prefix = &self.metric_prefix;

        // Cache request counters.
        output.push_str(
            "# HELP nginx_vts_cache_requests_total Total number of cache requests by status\n",
        );
        output.push_str("# TYPE nginx_vts_cache_requests_total counter\n");
        for zone_stats in cache_zones.values() {
            let zone = &zone_stats.name;
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
                    "{prefix}cache_requests_total{{zone=\"{zone}\",status=\"{status}\"}} {value}\n"
                ));
            }
        }
        output.push('\n');

        // Cache size gauges.
        output.push_str("# HELP nginx_vts_cache_size_bytes Cache size statistics in bytes\n");
        output.push_str("# TYPE nginx_vts_cache_size_bytes gauge\n");
        for zone_stats in cache_zones.values() {
            let zone = &zone_stats.name;
            output.push_str(&format!(
                "{prefix}cache_size_bytes{{zone=\"{zone}\",type=\"max\"}} {}\n",
                zone_stats.size.max_size
            ));
            output.push_str(&format!(
                "{prefix}cache_size_bytes{{zone=\"{zone}\",type=\"used\"}} {}\n",
                zone_stats.size.used_size
            ));
        }
        output.push('\n');

        // Cache hit ratio (derived from counters above).
        output.push_str("# HELP nginx_vts_cache_hit_ratio Cache hit ratio percentage\n");
        output.push_str("# TYPE nginx_vts_cache_hit_ratio gauge\n");
        for zone_stats in cache_zones.values() {
            let zone = &zone_stats.name;
            let hit_ratio = zone_stats.cache.hit_ratio();
            output.push_str(&format!(
                "{prefix}cache_hit_ratio{{zone=\"{zone}\"}} {hit_ratio:.2}\n"
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
    fn empty_cache_zones_emit_only_headers() {
        let empty: HashMap<String, CacheZoneStats> = HashMap::new();
        let out = PrometheusFormatter::new().format_cache_stats(&empty);
        assert!(out.contains("# HELP nginx_vts_cache_requests_total"));
        assert!(out.contains("# TYPE nginx_vts_cache_requests_total counter"));
        assert!(out.contains("# HELP nginx_vts_cache_size_bytes"));
        assert!(out.contains("# TYPE nginx_vts_cache_size_bytes gauge"));
        // No data lines.
        assert!(!out.contains("nginx_vts_cache_requests_total{"));
    }

    #[test]
    fn populated_cache_zone_renders_all_three_families() {
        let mut zones = HashMap::new();
        let mut zone = CacheZoneStats::new("test_cache");
        zone.cache.hit = 7;
        zone.cache.miss = 3;
        zone.size.max_size = 1_048_576;
        zone.size.used_size = 524_288;
        zones.insert("test_cache".into(), zone);

        let out = PrometheusFormatter::new().format_cache_stats(&zones);
        assert!(
            out.contains("nginx_vts_cache_requests_total{zone=\"test_cache\",status=\"hit\"} 7")
        );
        assert!(
            out.contains("nginx_vts_cache_requests_total{zone=\"test_cache\",status=\"miss\"} 3")
        );
        assert!(
            out.contains("nginx_vts_cache_size_bytes{zone=\"test_cache\",type=\"max\"} 1048576")
        );
        assert!(
            out.contains("nginx_vts_cache_size_bytes{zone=\"test_cache\",type=\"used\"} 524288")
        );
        // 7 / (7 + 3) = 70.00
        assert!(out.contains("nginx_vts_cache_hit_ratio{zone=\"test_cache\"} 70.00"));
    }
}
