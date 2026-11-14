//! vendor.oplus.hardware.displaypanelfeature 的 Rust HAL（纯 NDK 实现）。
//!
//! 不依赖 AIDL codegen / libbinder_rs，直接调 libbinder_ndk 的 C API。
//! 好处：只要 NDK + 目标机的 libbinder_ndk.so / liblog.so 就能 cargo build。
//!
//! 服务名必须和框架里硬编码的一致：
//!   KeyguardUtils.DISPLAY_PANEL_SERVICE_NAME =
//!     "vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default"

mod config;
mod cwb;
mod ndk;
mod panel;

use std::ffi::{c_char, c_void, CString};
use std::sync::OnceLock;

use ndk::*;

const DESCRIPTOR: &[u8] = b"vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature\0";
const SERVICE_NAME: &[u8] =
    b"vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default\0";

// AIDL 事务码（= 声明顺序），对着 IDisplayPanelFeature$Stub.onTransact 的
// `if-eq p1, 1/2/3` 核对过
const CODE_GET_VALUE: TransactionCode = 1;
const CODE_SET_VALUE: TransactionCode = 2;
const CODE_GET_INFO: TransactionCode = 3;
const CODE_GET_INTERFACE_VERSION: TransactionCode = 0x00ff_ffff;
const CODE_GET_INTERFACE_HASH: TransactionCode = 0x00ff_fffe;

/// AIDL 接口版本，跟 VINTF manifest 里的 <version>1</version> 对应
const INTERFACE_VERSION: i32 = 1;

static PANEL: OnceLock<panel::PanelFeature> = OnceLock::new();

// ---------------------------------------------------------------- logging

const LOG_TAG: &[u8] = b"DispPanelFeature\0";

struct LogSink;

impl log::Log for LogSink {
    fn enabled(&self, m: &log::Metadata) -> bool {
        m.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let prio = match record.level() {
            log::Level::Error => ANDROID_LOG_ERROR,
            log::Level::Warn => ANDROID_LOG_WARN,
            _ => ANDROID_LOG_INFO,
        };
        let line = format!("{}", record.args());
        let Ok(msg) = CString::new(line) else { return };
        unsafe {
            __android_log_print(
                prio,
                LOG_TAG.as_ptr() as *const c_char,
                b"%s\0".as_ptr() as *const c_char,
                msg.as_ptr(),
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: LogSink = LogSink;

// ---------------------------------------------------------------- binder 回调

unsafe extern "C" fn on_create(_args: *mut c_void) -> *mut c_void {
    // 必须返回非 null，否则 AIBinder_new 会失败
    1 as *mut c_void
}

unsafe extern "C" fn on_destroy(_user: *mut c_void) {}

unsafe extern "C" fn on_transact(
    _binder: *mut AIBinder,
    code: TransactionCode,
    input: *const AParcel,
    output: *mut AParcel,
) -> BinderStatus {
    let Some(panel) = PANEL.get() else {
        return STATUS_BAD_VALUE;
    };

    match code {
        CODE_GET_VALUE => {
            let mut feature_id: i32 = 0;
            if AParcel_readInt32(input, &mut feature_id) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            // out int[]：客户端只送长度，元素由我们回填
            let mut len: i32 = 0;
            if AParcel_readInt32(input, &mut len) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            let mut values: Vec<i32> = if len > 0 {
                vec![0; len as usize]
            } else {
                Vec::new()
            };

            let ret = panel.get_feature_value(feature_id, &mut values);

            if write_ok_header(output) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            AParcel_writeInt32(output, ret);
            AParcel_writeInt32Array(output, values.as_ptr(), values.len() as i32);
            STATUS_OK
        }

        CODE_SET_VALUE => {
            let mut feature_id: i32 = 0;
            if AParcel_readInt32(input, &mut feature_id) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            // in int[]：长度 + 元素
            let mut len: i32 = 0;
            if AParcel_readInt32(input, &mut len) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            let mut values: Vec<i32> = Vec::new();
            for _ in 0..len.max(0) {
                let mut v: i32 = 0;
                if AParcel_readInt32(input, &mut v) != STATUS_OK {
                    return STATUS_NOT_ENOUGH_DATA;
                }
                values.push(v);
            }

            let ret = panel.set_feature_value(feature_id, &values);

            if write_ok_header(output) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            AParcel_writeInt32(output, ret);
            STATUS_OK
        }

        CODE_GET_INFO => {
            let mut display_id: i32 = 0;
            if AParcel_readInt32(input, &mut display_id) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            let mut len: i32 = 0;
            if AParcel_readInt32(input, &mut len) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }

            let info = panel.get_info(display_id);
            let cstrings: Vec<CString> = info
                .iter()
                .map(|s| CString::new(s.as_str()).unwrap_or_default())
                .collect();

            if write_ok_header(output) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            AParcel_writeInt32(output, 0);
            AParcel_writeStringArray(
                output,
                &cstrings as *const Vec<CString> as *const c_void,
                cstrings.len() as i32,
                Some(string_getter),
            );
            STATUS_OK
        }

        CODE_GET_INTERFACE_VERSION => {
            if write_ok_header(output) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            AParcel_writeInt32(output, INTERFACE_VERSION);
            STATUS_OK
        }

        CODE_GET_INTERFACE_HASH => {
            if write_ok_header(output) != STATUS_OK {
                return STATUS_BAD_VALUE;
            }
            // 不参与校验，回空串即可（Java 侧 asInterface 不检查 hash）
            AParcel_writeString(output, b"\0".as_ptr() as *const c_char, 0);
            STATUS_OK
        }

        _ => STATUS_UNKNOWN_TRANSACTION,
    }
}

unsafe extern "C" fn string_getter(
    array: *const c_void,
    index: usize,
    out_len: *mut i32,
) -> *const c_char {
    let v = &*(array as *const Vec<CString>);
    match v.get(index) {
        Some(s) => {
            *out_len = s.as_bytes().len() as i32;
            s.as_ptr()
        }
        None => {
            *out_len = -1;
            std::ptr::null()
        }
    }
}

// ---------------------------------------------------------------- main

fn main() {
    let _ = log::set_logger(&LOGGER).map(|()| log::set_max_level(log::LevelFilter::Info));

    let cfg = config::Config::load(config::DEFAULT_CONF_PATH);
    if cfg.source.is_empty() {
        log::warn!("no config file found, using built-in defaults");
    } else {
        log::info!("config loaded from {}", cfg.source);
    }
    log::info!("0xd9 modes: {}", cfg.summary());
    log::info!("starting, config = {:?}", cfg);
    let _ = PANEL.set(panel::PanelFeature::new(cfg));

    unsafe {
        let class = AIBinder_Class_define(
            DESCRIPTOR.as_ptr() as *const c_char,
            Some(on_create),
            Some(on_destroy),
            Some(on_transact),
        );
        if class.is_null() {
            log::error!("AIBinder_Class_define failed");
            std::process::exit(1);
        }

        let binder = AIBinder_new(class, std::ptr::null_mut());
        if binder.is_null() {
            log::error!("AIBinder_new failed");
            std::process::exit(1);
        }

        let st = AServiceManager_addService(binder, SERVICE_NAME.as_ptr() as *const c_char);
        AIBinder_decStrong(binder);

        if st != STATUS_OK {
            log::error!("AServiceManager_addService failed: {}", st);
            std::process::exit(1);
        }
        log::info!(
            "registered {}",
            String::from_utf8_lossy(&SERVICE_NAME[..SERVICE_NAME.len() - 1])
        );

        // 同一进程再注册一个 cwb 服务：原机上它本来就内嵌在这个 HAL 里。
        // 只提供接口存在性，具体说明见 cwb.rs。
        if !cwb::register() {
            log::warn!("cwb service not registered, continuing anyway");
        }

        ABinderProcess_setThreadPoolMaxThreadCount(4);
        ABinderProcess_startThreadPool();
        log::info!("joining thread pool");
        ABinderProcess_joinThreadPool();
    }
}
