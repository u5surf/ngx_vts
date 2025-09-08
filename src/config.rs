//! Configuration structures for the VTS module

/// VTS module configuration structure
///
/// Contains settings for enabling status endpoint, zone tracking, and upstream statistics
#[repr(C)]
pub struct VtsConfig {
    /// Enable the VTS status endpoint
    pub enable_status: bool,
    /// Enable zone-based traffic tracking
    pub enable_zone: bool,
    /// Enable upstream statistics collection
    pub enable_upstream_stats: bool,
}

impl VtsConfig {
    /// Create a new VTS configuration with default settings
    pub fn new() -> Self {
        VtsConfig {
            enable_status: false,
            enable_zone: true,
            enable_upstream_stats: false,
        }
    }
}

impl Default for VtsConfig {
    fn default() -> Self {
        Self::new()
    }
}

// Required for ngx-rust module system
unsafe impl Send for VtsConfig {}
unsafe impl Sync for VtsConfig {}
