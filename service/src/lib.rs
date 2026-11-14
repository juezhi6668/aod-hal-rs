//! 只为了让 `cargo test` 能跑 src/config.rs 和 src/panel.rs 里的单测。
//!
//! Soong 编译时不看这个文件 —— Android.bp 里的 rust_binary 直接列了
//! src/main.rs / src/config.rs / src/panel.rs。
//!
//! 本地验证：
//!   cd service && cargo test
//!
//! 这两个模块只依赖 std + log，没有任何 binder / Android 依赖。

pub mod config;
pub mod panel;
