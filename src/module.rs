//! Module definition, directives and the status handler.
//!
//! This replaces `src/ngx_http_vts_module.c`. Everything here has an
//! equivalent in ngx-rust, so the C file it stands in for held no logic that
//! needed C - only the boilerplate of declaring a module, which ngx-rust's
//! `HttpModule` trait fills in.
//!
//! Moving the status handler over also removes an awkward FFI hop. The C
//! handler called back into Rust for the body, and the Rust side had to keep
//! the string alive across the return by parking a `CString` in a static
//! `Mutex` for the C code to read. Generating the body on the Rust side of the
//! handler makes that unnecessary.
//!
//! What is left in C is `src/ngx_vts_wrapper.c`: the log-phase handler, which
//! reads `r->upstream_states` and the cache fields that ngx-rust has no
//! accessors for. See `docs/ngx-rust-gaps.md`.

use core::ffi::{c_char, c_void};
use core::ptr;

use ngx::core::Status;
use ngx::ffi::{
    ngx_command_t, ngx_conf_set_flag_slot, ngx_conf_t, ngx_flag_t, ngx_http_module_t, ngx_int_t,
    ngx_module_t, ngx_parse_size, ngx_shared_memory_add, ngx_str_t, ngx_uint_t, NGX_CONF_FLAG,
    NGX_CONF_NOARGS, NGX_CONF_TAKE2, NGX_CONF_UNSET, NGX_HTTP_LOC_CONF, NGX_HTTP_LOC_CONF_OFFSET,
    NGX_HTTP_MAIN_CONF, NGX_HTTP_MAIN_CONF_OFFSET, NGX_HTTP_MODULE, NGX_HTTP_SRV_CONF,
    NGX_LOG_EMERG,
};
use ngx::http::{
    self, HttpModule, HttpModuleLocationConf, Merge, MergeConfigError, NgxHttpCoreModule,
};
use ngx::{http_request_handler, ngx_conf_log_error, ngx_string};

use crate::prometheus::generate_vts_status_content;

// The log-phase handler and its registration are still in C.
unsafe extern "C" {
    fn ngx_http_vts_init_wrapper(cf: *mut ngx_conf_t) -> ngx_int_t;
    fn vts_init_shm_zone(shm_zone: *mut ngx::ffi::ngx_shm_zone_t, data: *mut c_void) -> ngx_int_t;
}

struct Module;

/// The smallest zone worth accepting. A zone this size is 16 slab pages where
/// a page is 4k and only 16 where it is 64k, so the floor is stated in bytes
/// rather than pages deliberately: it is a guard against a typo, not a
/// guarantee that the zone will hold a given number of nodes.
const MIN_ZONE_SIZE: isize = 1024 * 1024;

/// Per-location configuration.
///
/// `enable` is an `ngx_flag_t` rather than a `bool` because
/// `ngx_conf_set_flag_slot` writes one at the offset the command table names,
/// and it starts at `NGX_CONF_UNSET` so that `merge` can tell "not set here"
/// from "set to off".
#[derive(Debug)]
pub struct LocConf {
    pub enable: ngx_flag_t,
}

impl Default for LocConf {
    fn default() -> Self {
        Self {
            enable: NGX_CONF_UNSET as ngx_flag_t,
        }
    }
}

unsafe impl HttpModuleLocationConf for Module {
    type LocationConf = LocConf;
}

impl Merge for LocConf {
    fn merge(&mut self, prev: &LocConf) -> Result<(), MergeConfigError> {
        if self.enable == NGX_CONF_UNSET as ngx_flag_t {
            self.enable = if prev.enable == NGX_CONF_UNSET as ngx_flag_t {
                0
            } else {
                prev.enable
            };
        }
        Ok(())
    }
}

impl HttpModule for Module {
    fn module() -> &'static ngx_module_t {
        // SAFETY: nginx only writes this static during initialization.
        unsafe { &*ptr::addr_of!(ngx_http_vts_module) }
    }

    /// Hands over to the C wrapper, which registers the log-phase handler.
    ///
    /// `add_phase_handler::<H>()` would do this from Rust, but the handler it
    /// would register is the one still written in C.
    unsafe extern "C" fn postconfiguration(cf: *mut ngx_conf_t) -> ngx_int_t {
        // SAFETY: nginx passes a valid `cf` to postconfiguration.
        unsafe { ngx_http_vts_init_wrapper(cf) }
    }
}

static mut NGX_HTTP_VTS_COMMANDS: [ngx_command_t; 4] = [
    ngx_command_t {
        name: ngx_string!("vts_zone"),
        type_: (NGX_HTTP_MAIN_CONF | NGX_CONF_TAKE2) as ngx_uint_t,
        set: Some(vts_zone_directive),
        conf: NGX_HTTP_MAIN_CONF_OFFSET,
        offset: 0,
        post: ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("vts_status"),
        type_: (NGX_HTTP_LOC_CONF | NGX_CONF_NOARGS) as ngx_uint_t,
        set: Some(vts_status_directive),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: 0,
        post: ptr::null_mut(),
    },
    ngx_command_t {
        name: ngx_string!("vts_upstream_stats"),
        type_: (NGX_HTTP_MAIN_CONF | NGX_HTTP_SRV_CONF | NGX_HTTP_LOC_CONF | NGX_CONF_FLAG)
            as ngx_uint_t,
        set: Some(ngx_conf_set_flag_slot),
        conf: NGX_HTTP_LOC_CONF_OFFSET,
        offset: core::mem::offset_of!(LocConf, enable),
        post: ptr::null_mut(),
    },
    ngx_command_t::empty(),
];

static NGX_HTTP_VTS_MODULE_CTX: ngx_http_module_t = ngx_http_module_t {
    preconfiguration: None,
    postconfiguration: Some(Module::postconfiguration),
    create_main_conf: None,
    init_main_conf: None,
    create_srv_conf: None,
    merge_srv_conf: None,
    create_loc_conf: Some(Module::create_loc_conf),
    merge_loc_conf: Some(Module::merge_loc_conf),
};

/// The module itself.
///
/// `src/ngx_vts_wrapper.c` still declares this `extern`, and nginx's generated
/// `ngx_modules.c` looks it up by name, so the mangling has to be off.
#[used]
#[allow(non_upper_case_globals)]
#[unsafe(no_mangle)]
pub static mut ngx_http_vts_module: ngx_module_t = ngx_module_t {
    ctx: &raw const NGX_HTTP_VTS_MODULE_CTX as _,
    commands: unsafe { &raw mut NGX_HTTP_VTS_COMMANDS[0] },
    type_: NGX_HTTP_MODULE as ngx_uint_t,
    ..ngx_module_t::default()
};

const PROMETHEUS_CONTENT_TYPE: ngx_str_t = ngx_string!("text/plain; version=0.0.4; charset=utf-8");

http_request_handler!(
    ngx_http_vts_status_handler,
    |request: &mut http::Request| {
        let raw = request.as_ref();

        if raw.method & (ngx::ffi::NGX_HTTP_GET | ngx::ffi::NGX_HTTP_HEAD) as ngx_uint_t == 0 {
            return Status(ngx::ffi::NGX_HTTP_NOT_ALLOWED as ngx_int_t);
        }

        // The log-phase handler looks for this context and skips the request, so
        // that scraping /status does not inflate the zone that serves it.
        request.set_module_ctx(ngx_http_vts_status_handler as *mut c_void, Module::module());

        let rc = request.discard_request_body();
        if rc != Status::NGX_OK {
            return rc;
        }

        let body = generate_vts_status_content();

        let Some(mut buf) = request.pool().create_buffer_from_str(&body) else {
            return http::HTTPStatus::INTERNAL_SERVER_ERROR.into();
        };

        request.set_status(http::HTTPStatus::OK);
        request.set_content_length_n(buf.len());

        // No accessor for these, so they are set on the raw struct.
        let raw = request.as_mut();
        raw.headers_out.content_type = PROMETHEUS_CONTENT_TYPE;
        raw.headers_out.content_type_len = PROMETHEUS_CONTENT_TYPE.len;
        raw.headers_out.content_type_lowcase = ptr::null_mut();

        use ngx::core::Buffer;
        buf.set_last_buf(request.is_main());
        buf.set_last_in_chain(true);

        let rc = request.send_header();
        if rc == Status::NGX_ERROR || rc > Status::NGX_OK || request.header_only() {
            return rc;
        }

        let mut out = ngx::ffi::ngx_chain_t {
            buf: buf.as_ngx_buf_mut(),
            next: ptr::null_mut(),
        };
        request.output_filter(&mut out)
    }
);

/// `vts_zone <name> <size>`
extern "C" fn vts_zone_directive(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    // SAFETY: configuration handlers always receive a valid `cf`.
    let cf = unsafe { cf.as_mut().unwrap() };

    // SAFETY: NGX_CONF_TAKE2 guarantees three elements, allocated from the
    // configuration pool.
    debug_assert!(!cf.args.is_null() && unsafe { (*cf.args).nelts >= 3 });
    let args = unsafe { (*cf.args).as_slice_mut() };

    let mut name = args[1];
    let size = unsafe { ngx_parse_size(&raw mut args[2]) };

    if size == -1 {
        ngx_conf_log_error!(
            NGX_LOG_EMERG,
            cf,
            "invalid size of vts_zone \"{}\"",
            unsafe { ngx::core::NgxStr::from_ngx_str(args[2]) }
        );
        return ngx::core::NGX_CONF_ERROR;
    }

    if size < MIN_ZONE_SIZE {
        ngx_conf_log_error!(
            NGX_LOG_EMERG,
            cf,
            "vts_zone \"{}\" is too small, minimum 1m",
            unsafe { ngx::core::NgxStr::from_ngx_str(args[1]) }
        );
        return ngx::core::NGX_CONF_ERROR;
    }

    // SAFETY: `cf` and `name` are valid; `name` points into the configuration
    // pool and so outlives the cycle.
    let shm_zone = unsafe {
        ngx_shared_memory_add(
            cf,
            &raw mut name,
            size as usize,
            (&raw mut ngx_http_vts_module).cast::<c_void>(),
        )
    };

    let Some(shm_zone) = (unsafe { shm_zone.as_mut() }) else {
        return ngx::core::NGX_CONF_ERROR;
    };

    shm_zone.init = Some(vts_init_shm_zone);

    ngx::core::NGX_CONF_OK
}

/// `vts_status`
extern "C" fn vts_status_directive(
    cf: *mut ngx_conf_t,
    _cmd: *mut ngx_command_t,
    _conf: *mut c_void,
) -> *mut c_char {
    // SAFETY: configuration handlers always receive a valid `cf`.
    let cf = unsafe { cf.as_mut().unwrap() };

    let Some(clcf) = NgxHttpCoreModule::location_conf_mut(cf) else {
        return ngx::core::NGX_CONF_ERROR;
    };
    clcf.handler = Some(ngx_http_vts_status_handler);

    ngx::core::NGX_CONF_OK
}
