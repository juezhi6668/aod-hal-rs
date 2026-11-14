//! 纯逻辑层：不依赖 binder，可以单独用 cargo 跑单测。
//!
//! 实现的 feature：
//!   0x07  AOD 灯模式（OMMDPAOD）        → aod_light_mode_set 节点
//!   0xC0  AOD 区域（局部刷新）          → aod_area 节点，格式原样透传
//!   0xD9  AOD 显示状态 / 能力位          → 由配置决定返回什么
//!
//! 其它 id 一律「返回 0 + 填 0」，保证调用方拿到的数组长度和它申请的一致。

use std::fs;
use std::sync::atomic::{AtomicI32, Ordering};

use crate::config::Config;

/// AOD 灯模式（= HAL 里的 OMMDPAOD）
pub const FEATURE_AOD_LIGHT_MODE: i32 = 0x07;
/// AOD 区域（局部刷新区域，最多 8 个 int，格式透传给内核）
pub const FEATURE_AOD_AREA: i32 = 0xC0;
/// AOD 显示状态 / 能力位（SystemUI 用它判断能不能开灵感接续 / 全景息屏）
pub const FEATURE_AOD_DISPLAY_STATE: i32 = 0xD9;

// 0xd9 的位定义，和 SystemUI 的判断一一对应
/// bit0：屏灭时不要直接进 doze（SystemUI 里是反逻辑）
#[allow(dead_code)]
pub const AOD_BIT_NO_DOZE_WITH_OFF: i32 = 0x1;
/// bit1：灵感接续（workshop AOD）
#[allow(dead_code)]
pub const AOD_BIT_INSPIRATION: i32 = 0x2;
/// bit2：全景息屏（全屏 AOD）
#[allow(dead_code)]
pub const AOD_BIT_PANORAMIC: i32 = 0x4;

pub struct PanelFeature {
    cfg: Config,
    light_mode: AtomicI32,
}

impl PanelFeature {
    pub fn new(cfg: Config) -> Self {
        Self {
            cfg,
            // 默认认为 AOD 灯是开的
            light_mode: AtomicI32::new(1),
        }
    }

    #[allow(dead_code)]
    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// 对应 AIDL `int getDisplayPanelFeatureValue(int featureId, out int[] values)`
    ///
    /// values 的长度由调用方决定（Java 侧 `new int[n]`），
    /// 我们只覆盖元素值，长度不动，避免数组越界。
    pub fn get_feature_value(&self, feature_id: i32, values: &mut Vec<i32>) -> i32 {
        match feature_id {
            FEATURE_AOD_DISPLAY_STATE => {
                let bits = self.cfg.aod_support_bits;
                log::info!(
                    "get 0xd9 -> {:#x} (inspiration={} panoramic={})",
                    bits,
                    bits & AOD_BIT_INSPIRATION != 0,
                    bits & AOD_BIT_PANORAMIC != 0
                );
                Self::fill(values, &[bits]);
            }
            FEATURE_AOD_LIGHT_MODE => {
                let v = self.light_mode.load(Ordering::Relaxed);
                log::info!("get 0x07 -> {}", v);
                Self::fill(values, &[v]);
            }
            FEATURE_AOD_AREA => {
                log::info!("get 0xc0 (aod area) len={}", values.len());
                Self::fill(values, &[0]);
            }
            other => {
                // 指纹高亮之类的其它 feature 这里不实现，给 0 而不是失败，
                // 免得调用方拿到 null 数组。
                log::warn!("get unknown feature {:#x}, return zeros", other);
                Self::fill(values, &[0]);
            }
        }
        0
    }

    /// 对应 AIDL `int setDisplayPanelFeatureValue(int featureId, in int[] values)`
    pub fn set_feature_value(&self, feature_id: i32, values: &[i32]) -> i32 {
        match feature_id {
            FEATURE_AOD_LIGHT_MODE => {
                let v = values.first().copied().unwrap_or(0);
                self.light_mode.store(v, Ordering::Relaxed);
                log::info!("set 0x07 aod light mode = {}", v);
                self.write_node(&self.cfg.aod_light_mode_node, &[v])
            }
            FEATURE_AOD_DISPLAY_STATE => {
                // 0 = 经典, 2 = 工坊/灵感, 4 = 全景(全屏)
                log::info!("set 0xd9 aod display state = {:?}", values);
                0
            }
            FEATURE_AOD_AREA => {
                // 区域格式由上层决定（SystemUI 传什么就写什么），HAL 只负责透传
                log::info!("set 0xc0 aod area = {:?}", values);
                if values.is_empty() {
                    return 0;
                }
                self.write_node(&self.cfg.aod_area_node, values)
            }
            other => {
                log::warn!("set unknown feature {:#x} = {:?}", other, values);
                0
            }
        }
    }

    /// 对应 AIDL `int getDisplayPanelInfo(int displayId, out String[] info)`
    pub fn get_info(&self, display_id: i32) -> Vec<String> {
        let bits = self.cfg.aod_support_bits;
        vec![
            self.cfg.panel_name.clone(),
            self.cfg.panel_id.to_string(),
            display_id.to_string(),
            format!("aod_support_bits={:#x}", bits),
        ]
    }

    fn fill(values: &mut Vec<i32>, src: &[i32]) {
        if values.is_empty() {
            values.extend_from_slice(src);
            return;
        }
        for (i, v) in values.iter_mut().enumerate() {
            *v = src.get(i).copied().unwrap_or(0);
        }
    }

    fn write_node(&self, path: &str, values: &[i32]) -> i32 {
        let payload = values
            .iter()
            .map(|v| v.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        match fs::write(path, payload + "\n") {
            Ok(()) => 0,
            Err(e) => {
                log::warn!("write {} failed: {}", path, e);
                if self.cfg.strict {
                    -1
                } else {
                    0
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BIT_INSPIRATION, BIT_PANORAMIC};

    fn svc(bits: i32) -> PanelFeature {
        let mut cfg = Config::default();
        cfg.aod_support_bits = bits;
        PanelFeature::new(cfg)
    }

    #[test]
    fn aod_state_reports_bits() {
        let s = svc(BIT_PANORAMIC);
        let mut buf = vec![0i32; 1];
        assert_eq!(s.get_feature_value(FEATURE_AOD_DISPLAY_STATE, &mut buf), 0);
        assert_eq!(buf, vec![BIT_PANORAMIC]);
    }

    #[test]
    fn inspiration_bit_is_reported() {
        let s = svc(BIT_INSPIRATION | BIT_PANORAMIC);
        let mut buf = vec![0i32; 1];
        s.get_feature_value(FEATURE_AOD_DISPLAY_STATE, &mut buf);
        assert_eq!(buf[0] & BIT_INSPIRATION, BIT_INSPIRATION);
        assert_eq!(buf[0] & BIT_PANORAMIC, BIT_PANORAMIC);
    }

    #[test]
    fn keeps_caller_buffer_length() {
        let s = svc(0xC);
        let mut buf = vec![0i32; 8];
        s.get_feature_value(FEATURE_AOD_AREA, &mut buf);
        assert_eq!(buf.len(), 8);
    }

    #[test]
    fn empty_buffer_gets_one_element() {
        let s = svc(BIT_PANORAMIC);
        let mut buf: Vec<i32> = Vec::new();
        s.get_feature_value(FEATURE_AOD_DISPLAY_STATE, &mut buf);
        assert_eq!(buf, vec![BIT_PANORAMIC]);
    }

    #[test]
    fn missing_node_is_not_fatal_in_stub_mode() {
        let mut cfg = Config::default();
        cfg.aod_light_mode_node = "/definitely/not/here".to_string();
        cfg.aod_area_node = "/definitely/not/here/either".to_string();
        cfg.strict = false;
        let s = PanelFeature::new(cfg);
        assert_eq!(s.set_feature_value(FEATURE_AOD_LIGHT_MODE, &[1]), 0);
        assert_eq!(s.set_feature_value(FEATURE_AOD_AREA, &[1, 2, 3, 4]), 0);
    }

    #[test]
    fn info_reports_bits() {
        let s = svc(BIT_INSPIRATION | BIT_PANORAMIC);
        let info = s.get_info(0);
        assert_eq!(info[0], "oplus_panel");
        assert!(info.iter().any(|s| s == "aod_support_bits=0x6"));
    }
}
