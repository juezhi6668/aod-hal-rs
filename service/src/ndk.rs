//! libbinder_ndk + liblog 的最小 FFI 绑定。
//!
//! 不用 aidl codegen、不用 bindgen、不用 libbinder_rs —— 直接对 NDK 的 C API 下手，
//! 这样在只有 NDK 的环境（没有 AOSP 源码树）里也能 cargo build 出真机可用的二进制。
//!
//! 符号签名逐条对着 NDK r26d 的头文件核对过：
//!   sysroot/usr/include/android/binder_ibinder.h
//!   sysroot/usr/include/android/binder_parcel.h
//!   sysroot/usr/include/android/binder_status.h

#![allow(non_camel_case_types, non_snake_case, dead_code)]

use std::ffi::{c_char, c_void};

pub type BinderStatus = i32;
pub type TransactionCode = u32;

pub const STATUS_OK: BinderStatus = 0;
/// -EBADMSG
pub const STATUS_UNKNOWN_TRANSACTION: BinderStatus = -74;
/// -EINVAL
pub const STATUS_BAD_VALUE: BinderStatus = -22;
/// -ENODATA
pub const STATUS_NOT_ENOUGH_DATA: BinderStatus = -61;

pub type AIBinder = c_void;
pub type AIBinderClass = c_void;
pub type AParcel = c_void;
pub type AStatus = c_void;

pub type OnCreate = unsafe extern "C" fn(*mut c_void) -> *mut c_void;
pub type OnDestroy = unsafe extern "C" fn(*mut c_void);
pub type OnTransact = unsafe extern "C" fn(
    *mut AIBinder,
    TransactionCode,
    *const AParcel,
    *mut AParcel,
) -> BinderStatus;

/// typedef const char* (*AParcel_stringArrayElementGetter)(const void*, size_t, int32_t*);
pub type StringArrayElementGetter =
    unsafe extern "C" fn(*const c_void, usize, *mut i32) -> *const c_char;

#[link(name = "binder_ndk")]
extern "C" {
    pub fn AIBinder_Class_define(
        descriptor: *const c_char,
        on_create: Option<OnCreate>,
        on_destroy: Option<OnDestroy>,
        on_transact: Option<OnTransact>,
    ) -> *mut AIBinderClass;

    pub fn AIBinder_new(class: *const AIBinderClass, args: *mut c_void) -> *mut AIBinder;
    pub fn AIBinder_decStrong(binder: *mut AIBinder);

    /// 这个符号在 NDK 的 stub libbinder_ndk.so 里没有，
    /// 但真机 /system/lib64/libbinder_ndk.so 一定有。
    pub fn AServiceManager_addService(
        binder: *mut AIBinder,
        instance: *const c_char,
    ) -> BinderStatus;

    pub fn ABinderProcess_startThreadPool();
    pub fn ABinderProcess_joinThreadPool();
    pub fn ABinderProcess_setThreadPoolMaxThreadCount(num: u32) -> bool;

    pub fn AStatus_newOk() -> *mut AStatus;
    pub fn AStatus_delete(status: *mut AStatus);

    pub fn AParcel_writeStatusHeader(parcel: *mut AParcel, status: *const AStatus) -> BinderStatus;
    pub fn AParcel_readInt32(parcel: *const AParcel, value: *mut i32) -> BinderStatus;
    pub fn AParcel_writeInt32(parcel: *mut AParcel, value: i32) -> BinderStatus;
    pub fn AParcel_writeInt32Array(
        parcel: *mut AParcel,
        array: *const i32,
        length: i32,
    ) -> BinderStatus;
    pub fn AParcel_writeStringArray(
        parcel: *mut AParcel,
        array: *const c_void,
        length: i32,
        getter: Option<StringArrayElementGetter>,
    ) -> BinderStatus;
    pub fn AParcel_writeString(
        parcel: *mut AParcel,
        string: *const c_char,
        length: i32,
    ) -> BinderStatus;
}

/// AIDL 每个 reply 都要求先写一个 status header（Java 侧对应 readException()）。
pub unsafe fn write_ok_header(out: *mut AParcel) -> BinderStatus {
    let ok = AStatus_newOk();
    if ok.is_null() {
        return STATUS_BAD_VALUE;
    }
    let rc = AParcel_writeStatusHeader(out, ok);
    AStatus_delete(ok);
    rc
}

#[link(name = "log")]
extern "C" {
    pub fn __android_log_print(prio: i32, tag: *const c_char, fmt: *const c_char, ...) -> i32;
}

pub const ANDROID_LOG_INFO: i32 = 4;
pub const ANDROID_LOG_WARN: i32 = 5;
pub const ANDROID_LOG_ERROR: i32 = 6;
