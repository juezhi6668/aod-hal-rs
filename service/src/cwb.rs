//! `vendor.oplus.hardware.cwb.ICwbService` 的最小注册实现。
//!
//! 为什么加这个
//! ------------
//! `/system_ext/bin/theia_screen_monitor` 一直在按 3.5 次/秒的频率请求
//! `vendor.oplus.hardware.cwb.ICwbService/default`，但本 ROM 没有这个 HAL，
//! servicemanager 只能反复回：
//!
//!   Tried to start aidl service vendor.oplus.hardware.cwb.ICwbService/default
//!   as a lazy service, but was unable to.
//!
//! 十秒就能刷出两千多条。原机上这个服务其实是内嵌在
//! `vendor.oplus.hardware.displaypanelfeature-service` 里的（同一进程同时
//! 注册 displaypanelfeature 和 cwb 两个接口），所以这里照同样的做法补上。
//!
//! 实现范围
//! --------
//! cwb 的 AIDL 有 16 个方法，还牵涉 Rect / Error / Rgb / Histogram 四个
//! 自定义 parcelable 和共享内存（ScopedFileDescriptor）。本 ROM 里没有它的
//! Java stub，parcelable 布局无法可靠推导，所以这里只做「服务存在性」：
//!
//!   * getInterfaceVersion / getInterfaceHash  → 正常应答
//!   * 其余方法                                → STATUS_UNKNOWN_TRANSACTION
//!
//! 目的很明确：让客户端拿到一个真实的 binder，止住无限重试；
//! 不去猜 parcelable 布局，避免客户端拿到半截数据直接崩。

use std::ffi::{c_char, c_void};

use crate::ndk::*;

const DESCRIPTOR: &[u8] = b"vendor.oplus.hardware.cwb.ICwbService\0";
const SERVICE_NAME: &[u8] = b"vendor.oplus.hardware.cwb.ICwbService/default\0";

/// VINTF manifest 里声明的是 <version>2</version>
const INTERFACE_VERSION: i32 = 2;

const CODE_GET_INTERFACE_VERSION: TransactionCode = 0x00ff_ffff;
const CODE_GET_INTERFACE_HASH: TransactionCode = 0x00ff_fffe;

unsafe extern "C" fn on_create(_args: *mut c_void) -> *mut c_void {
    // 必须非 null，否则 AIBinder_new 会失败
    1 as *mut c_void
}

unsafe extern "C" fn on_destroy(_user: *mut c_void) {}

unsafe extern "C" fn on_transact(
    _binder: *mut AIBinder,
    code: TransactionCode,
    _input: *const AParcel,
    output: *mut AParcel,
) -> BinderStatus {
    match code {
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
            AParcel_writeString(output, b"\0".as_ptr() as *const c_char, 0);
            STATUS_OK
        }

        _ => {
            log::debug!("cwb: 未实现的事务 code={}", code);
            STATUS_UNKNOWN_TRANSACTION
        }
    }
}

/// 注册 cwb 服务。调用前 binder 线程池必须还没 join。
pub unsafe fn register() -> bool {
    let class = AIBinder_Class_define(
        DESCRIPTOR.as_ptr() as *const c_char,
        Some(on_create),
        Some(on_destroy),
        Some(on_transact),
    );
    if class.is_null() {
        log::error!("cwb: AIBinder_Class_define failed");
        return false;
    }

    let binder = AIBinder_new(class, std::ptr::null_mut());
    if binder.is_null() {
        log::error!("cwb: AIBinder_new failed");
        return false;
    }

    let st = AServiceManager_addService(binder, SERVICE_NAME.as_ptr() as *const c_char);
    AIBinder_decStrong(binder);

    if st != STATUS_OK {
        log::error!("cwb: AServiceManager_addService failed: {}", st);
        return false;
    }

    log::info!(
        "registered {}",
        String::from_utf8_lossy(&SERVICE_NAME[..SERVICE_NAME.len() - 1])
    );
    true
}
