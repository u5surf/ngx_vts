//! Prometheus metrics formatter for VTS statistics.
//!
//! The formatter is split by metric family across submodules:
//!
//!   - [`connections`] — `nginx_vts_connections` and `_connections_total`
//!   - [`server`]      — `nginx_vts_server_*`
//!   - [`upstream`]    — `nginx_vts_upstream_*` (counters + histogram)
//!   - [`cache`]       — `nginx_vts_cache_*`
//!
//! [`PrometheusFormatter::format_nginx_info`] and the top-level
//! [`generate_vts_status_content`] entry point live in this module
//! because they orchestrate the others.

use std::collections::HashMap;

use crate::upstream_stats::UpstreamZone;

#[cfg(not(test))]
use ngx::ffi::ngx_time;

mod cache;
mod connections;
mod server;
mod upstream;

/// Prometheus metrics formatter for VTS statistics.
///
/// Carries the metric-name prefix and provides a `format_*` method
/// per metric family.  The method bodies live in the submodules
/// listed above; this file holds the type and the `nginx_info`
/// metric (which is the only one that doesn't take a per-zone map).
#[allow(dead_code)] // All fields used in formatting
pub struct PrometheusFormatter {
    /// Optional metric prefix (default: "nginx_vts_")
    pub metric_prefix: String,
}

impl PrometheusFormatter {
    /// Create a new Prometheus formatter with default settings
    pub fn new() -> Self {
        Self {
            metric_prefix: "nginx_vts_".to_string(),
        }
    }

    /// Create a new Prometheus formatter with custom metric prefix
    #[allow(dead_code)] // Used in tests and future integrations
    pub fn with_prefix(prefix: &str) -> Self {
        Self {
            metric_prefix: prefix.to_string(),
        }
    }

    /// Format nginx basic info metrics into Prometheus format
    pub fn format_nginx_info(&self, hostname: &str, version: &str) -> String {
        let mut output = String::new();
        output.push_str(&format!(
            "# HELP {}info Nginx VTS module information\n",
            self.metric_prefix
        ));
        output.push_str(&format!("# TYPE {}info gauge\n", self.metric_prefix));
        output.push_str(&format!(
            "{}info{{hostname=\"{}\",version=\"{}\"}} 1\n\n",
            self.metric_prefix, hostname, version
        ));
        output
    }
}

impl Default for PrometheusFormatter {
    fn default() -> Self {
        Self::new()
    }
}

/// Generate VTS status content.
///
/// Creates a comprehensive status report including server
/// information, connection statistics, and request metrics.
pub fn generate_vts_status_content() -> String {
    // Collect current nginx connection statistics only in production
    #[cfg(not(test))]
    crate::vts_collect_nginx_connections();

    let manager = crate::VTS_MANAGER
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let formatter = PrometheusFormatter::new();

    // When `vts_zone` is configured the cross-worker shared table is the
    // authoritative source for server and upstream stats. Otherwise we
    // fall back to the process-local manager (used by unit tests and by
    // single-worker development setups that haven't declared a zone).
    let server_zone_stats =
        crate::shm::snapshot_servers().unwrap_or_else(|| manager.get_all_server_stats());
    let upstream_owned = crate::shm::snapshot_upstreams();
    let upstream_zones: &HashMap<String, UpstreamZone> = match upstream_owned.as_ref() {
        Some(m) => m,
        None => manager.get_all_upstream_zones(),
    };

    let mut content = String::new();

    // Header information
    content.push_str(&format!(
        "# nginx-vts-rust\n\
         # Version: {}\n\
         # Hostname: {}\n\
         # Current Time: {}\n\
         \n\
         # VTS Status: Active\n\
         # Module: nginx-vts-rust\n\
         \n",
        env!("CARGO_PKG_VERSION"),
        get_hostname(),
        get_current_time()
    ));

    content.push_str("# Prometheus Metrics:\n");

    content.push_str(&formatter.format_nginx_info(&get_hostname(), env!("CARGO_PKG_VERSION")));
    content.push_str(&formatter.format_connection_stats(manager.get_connection_stats()));
    content.push_str(&formatter.format_server_stats(&server_zone_stats));

    if !upstream_zones.is_empty() {
        content.push_str(&formatter.format_upstream_stats(upstream_zones));
    } else {
        // Placeholder for when no upstream zones exist.
        content.push_str(
            "# HELP nginx_vts_upstream_zones_total Total number of upstream zones\n\
             # TYPE nginx_vts_upstream_zones_total gauge\n\
             nginx_vts_upstream_zones_total 0\n\n",
        );
    }

    // Generate cache metrics — prefer the cross-worker shared table
    // when configured, otherwise fall back to the process-local manager.
    let cache_zones = crate::shm::snapshot_caches().unwrap_or_else(crate::get_all_cache_zones);
    content.push_str(&formatter.format_cache_stats(&cache_zones));

    content
}

/// Get system hostname (nginx-independent version for testing).
pub fn get_hostname() -> String {
    #[cfg(not(test))]
    {
        let mut buf = [0u8; 256];
        unsafe {
            if libc::gethostname(buf.as_mut_ptr() as *mut libc::c_char, buf.len()) == 0 {
                let len = buf.iter().position(|&x| x == 0).unwrap_or(buf.len());
                if let Ok(hostname_str) = std::str::from_utf8(&buf[..len]) {
                    return hostname_str.to_string();
                }
            }
        }
        "localhost".to_string()
    }

    #[cfg(test)]
    {
        "test-hostname".to_string()
    }
}

/// Get current time as string (nginx-independent version for testing).
pub fn get_current_time() -> String {
    #[cfg(not(test))]
    {
        let current_time = ngx_time();
        format!("{current_time}")
    }

    #[cfg(test)]
    {
        "1234567890".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formatter_creation_uses_default_prefix() {
        let f = PrometheusFormatter::new();
        assert_eq!(f.metric_prefix, "nginx_vts_");
    }

    #[test]
    fn formatter_with_prefix_overrides_default() {
        let f = PrometheusFormatter::with_prefix("custom_");
        assert_eq!(f.metric_prefix, "custom_");
    }

    #[test]
    fn format_nginx_info_includes_hostname_and_version() {
        let out = PrometheusFormatter::new().format_nginx_info("h.example.test", "1.2.3");
        assert!(out.contains("# HELP nginx_vts_info Nginx VTS module information"));
        assert!(out.contains("# TYPE nginx_vts_info gauge"));
        assert!(out.contains("nginx_vts_info{hostname=\"h.example.test\",version=\"1.2.3\"} 1"));
    }
}
