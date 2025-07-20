//! Configuration structures for the VTS module

/// VTS module configuration structure
///
/// Contains settings for enabling status endpoint and zone tracking
#[repr(C)]
pub struct VtsConfig {
    /// Enable the VTS status endpoint
    pub enable_status: bool,
    /// Enable zone-based traffic tracking
    pub enable_zone: bool,
}

impl VtsConfig {
    /// Create a new VTS configuration with default settings
    pub fn new() -> Self {
        VtsConfig {
            enable_status: false,
            enable_zone: true,
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
