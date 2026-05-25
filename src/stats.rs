//! Per-server-zone view types shared by the shared-memory backend
//! (`shm.rs`), the process-local fallback (`vts_node.rs`), and the
//! Prometheus formatter (`prometheus.rs`).
//!
//! Storage layers populate these structs at snapshot time; the
//! formatter reads them.  Field shapes match what
//! `nginx_vts_server_*` metrics need.

/// Per-status-class response counters.
#[derive(Debug, Clone, Default)]
pub struct VtsResponseStats {
    /// 1xx responses.
    pub status_1xx: u64,
    /// 2xx responses.
    pub status_2xx: u64,
    /// 3xx responses.
    pub status_3xx: u64,
    /// 4xx responses.
    pub status_4xx: u64,
    /// 5xx responses.
    pub status_5xx: u64,
}

/// Request-time aggregate (in seconds).
#[derive(Debug, Clone, Default)]
pub struct VtsRequestTimes {
    /// Sum of all observed request times.  Populated by the storage
    /// layers; reserved for a future `..._seconds_sum` Prometheus
    /// metric and otherwise unread today.
    #[allow(dead_code)]
    pub total: f64,
    /// Minimum observed request time (`0.0` if no requests yet).
    pub min: f64,
    /// Maximum observed request time.
    pub max: f64,
    /// Arithmetic mean (`total / count`).
    pub avg: f64,
}

/// Snapshot of one server zone (`server_name` from the matched
/// server block).  Aggregates everything the formatter needs to
/// render `nginx_vts_server_*` metrics for a single zone.
#[derive(Debug, Clone, Default)]
pub struct VtsServerStats {
    /// Total requests served by this zone.
    pub requests: u64,
    /// Bytes received from clients.
    pub bytes_in: u64,
    /// Bytes sent to clients.
    pub bytes_out: u64,
    /// Per-status-class response breakdown.
    pub responses: VtsResponseStats,
    /// Request-time aggregate.
    pub request_times: VtsRequestTimes,
}

/// Connection-state snapshot used by the Prometheus
/// `nginx_vts_connections` series.
#[derive(Debug, Clone, Default)]
pub struct VtsConnectionStats {
    /// Currently active connections.
    pub active: u64,
    /// Connections reading request headers.
    pub reading: u64,
    /// Connections writing response data.
    pub writing: u64,
    /// Idle connections waiting for requests.
    pub waiting: u64,
    /// Total accepted connections.
    pub accepted: u64,
    /// Total handled connections.
    pub handled: u64,
}
