use ngx::ffi::*;
use std::os::raw::c_void;

#[repr(C)]
pub struct VtsConfig {
    pub enable_status: bool,
    pub enable_zone: bool,
}

impl VtsConfig {
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
