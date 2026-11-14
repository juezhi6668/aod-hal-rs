//! 极简 key=value 配置解析，不引第三方 crate。
//!
//! 查找顺序：
//!   1) /vendor/etc/displaypanelfeature.conf      （正式位置，需要 root 才能写）
//!   2) /data/local/tmp/displaypanelfeature.conf （adb push 就能写，免 root 调试用）
//!   3) 内置默认值
//!
//! 默认值必须同时包含灵感接续和全景息屏 —— 早期版本只给了 0x4，
//! 结果设备上没推配置文件时「灵感接续」永远不出现。

use std::fs;

pub const DEFAULT_CONF_PATH: &str = "/vendor/etc/displaypanelfeature.conf";
pub const FALLBACK_CONF_PATH: &str = "/data/local/tmp/displaypanelfeature.conf";

// ---- 0xd9 的位定义（和 SystemUI 里的判断一一对应）----
/// bit0：屏灭时不要直接进 doze（SystemUI 里是反逻辑，置 1 表示 gotoDozeWithOff = false）
pub const BIT_NO_DOZE_WITH_OFF: i32 = 0x1;
/// bit1：灵感接续（workshop AOD）。
/// SystemUI 里叫 isSupportSmoothTransition，会写 Settings.Secure
/// "oplus_keyguard_workshop_aod_support"；
/// Aod.apk 的 isSupportInspirationAod() 就是读这个值决定显不显示「灵感接续」。
pub const BIT_INSPIRATION: i32 = 0x2;
/// bit2：全景息屏（全屏 AOD），对应 Settings.Secure
/// "oplus_keyguard_panoramic_aod_support"
pub const BIT_PANORAMIC: i32 = 0x4;

/// 默认同时开灵感接续 + 全景息屏。
/// 没推配置文件时也必须让两个入口都出来，不能只给 0x4。
pub const DEFAULT_AOD_SUPPORT_BITS: i32 = BIT_INSPIRATION | BIT_PANORAMIC;

#[derive(Debug, Clone)]
pub struct Config {
    /// feature 0xd9 的返回值（能力位掩码），最终值
    pub aod_support_bits: i32,
    /// feature 7 要写的节点
    pub aod_light_mode_node: String,
    /// feature 0xc0 要写的节点（AOD 局部刷新区域，格式透传）
    pub aod_area_node: String,
    /// 面板名字，仅供 getDisplayPanelInfo 用
    pub panel_name: String,
    pub panel_id: i32,
    /// true = 节点写失败时返回 -1；false = 一律返回 0（stub 模式，推荐）
    pub strict: bool,
    /// 实际读到配置的文件路径，空串表示用的默认值（给日志用）
    pub source: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            aod_support_bits: DEFAULT_AOD_SUPPORT_BITS,
            aod_light_mode_node: "/sys/kernel/oplus_display/aod_light_mode_set".to_string(),
            aod_area_node: "/sys/kernel/oplus_display/aod_area".to_string(),
            panel_name: "oplus_panel".to_string(),
            panel_id: 0,
            strict: false,
            source: String::new(),
        }
    }
}

impl Config {
    pub fn load(path: &str) -> Self {
        for p in [path, FALLBACK_CONF_PATH] {
            if let Ok(text) = fs::read_to_string(p) {
                let mut cfg = Self::parse(&text);
                cfg.source = p.to_string();
                return cfg;
            }
        }
        Self::default()
    }

    /// 人类可读的能力位说明，给启动日志 / 自检用
    pub fn summary(&self) -> String {
        let b = self.aod_support_bits;
        format!(
            "inspiration={} panoramic={} no_doze_with_off={} (bits={:#x})",
            b & BIT_INSPIRATION != 0,
            b & BIT_PANORAMIC != 0,
            b & BIT_NO_DOZE_WITH_OFF != 0,
            b
        )
    }

    fn parse(text: &str) -> Self {
        let mut cfg = Config::default();

        // 三个 enable_* 是「友好开关」，只要出现任意一个就接管 aod_support_bits
        let mut named: Option<i32> = None;
        let mut raw_bits: Option<i32> = None;

        for raw in text.lines() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((k, v)) = line.split_once('=') else {
                continue;
            };
            let (k, v) = (k.trim(), v.trim());

            match k {
                "enable_inspiration" => {
                    let b = named.get_or_insert(0);
                    if truthy(v) {
                        *b |= BIT_INSPIRATION;
                    }
                }
                "enable_panoramic" => {
                    let b = named.get_or_insert(0);
                    if truthy(v) {
                        *b |= BIT_PANORAMIC;
                    }
                }
                "enable_no_doze_with_off" => {
                    let b = named.get_or_insert(0);
                    if truthy(v) {
                        *b |= BIT_NO_DOZE_WITH_OFF;
                    }
                }
                "aod_support_bits" => {
                    if let Some(n) = parse_int(v) {
                        raw_bits = Some(n);
                    }
                }
                "aod_light_mode_node" => cfg.aod_light_mode_node = v.to_string(),
                "aod_area_node" => cfg.aod_area_node = v.to_string(),
                "panel_name" => cfg.panel_name = v.to_string(),
                "panel_id" => {
                    if let Some(n) = parse_int(v) {
                        cfg.panel_id = n;
                    }
                }
                "strict" => cfg.strict = truthy(v),
                _ => {}
            }
        }

        // 友好开关优先；没写才用原始掩码
        if let Some(bits) = named {
            cfg.aod_support_bits = bits;
        } else if let Some(bits) = raw_bits {
            cfg.aod_support_bits = bits;
        }

        cfg
    }
}

fn truthy(s: &str) -> bool {
    matches!(
        s.trim().to_ascii_lowercase().as_str(),
        "1" | "true" | "yes" | "on"
    )
}

fn parse_int(s: &str) -> Option<i32> {
    let s = s.trim();
    if let Some(hex) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        i32::from_str_radix(hex, 16).ok()
    } else if let Some(bin) = s.strip_prefix("0b").or_else(|| s.strip_prefix("0B")) {
        i32::from_str_radix(bin, 2).ok()
    } else {
        s.parse::<i32>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_tmp(name: &str, body: &str) -> String {
        let mut p = std::env::temp_dir();
        p.push(format!("dpf_test_{}_{}.conf", name, std::process::id()));
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(body.as_bytes()).unwrap();
        p.to_string_lossy().into_owned()
    }

    #[test]
    fn parses_hex_bin_dec() {
        assert_eq!(parse_int("0x4"), Some(4));
        assert_eq!(parse_int("0xC"), Some(12));
        assert_eq!(parse_int("0b1010"), Some(10));
        assert_eq!(parse_int("12"), Some(12));
        assert_eq!(parse_int("oops"), None);
    }

    /// 回归测试：早期版本默认值只有 0x4，设备上没推配置文件时
    /// 「灵感接续」永远不会出现。默认值必须两个都开。
    #[test]
    fn default_enables_inspiration_and_panoramic() {
        let cfg = Config::default();
        assert_eq!(cfg.aod_support_bits, 0x6);
        assert_eq!(
            cfg.aod_support_bits,
            BIT_INSPIRATION | BIT_PANORAMIC
        );
        assert!(cfg.source.is_empty());
    }

    #[test]
    fn named_switches_compose_bits() {
        let p = write_tmp("named", "enable_inspiration = 1\nenable_panoramic = 1\n");
        let cfg = Config::load(&p);
        assert_eq!(cfg.aod_support_bits, BIT_INSPIRATION | BIT_PANORAMIC);
        let _ = fs::remove_file(p);
    }

    #[test]
    fn raw_mask_used_when_no_named_switch() {
        let p = write_tmp("raw", "aod_support_bits = 0x6\n");
        let cfg = Config::load(&p);
        assert_eq!(cfg.aod_support_bits, 0x6);
        let _ = fs::remove_file(p);
    }

    #[test]
    fn named_switch_wins_over_raw_mask() {
        let p = write_tmp("both", "aod_support_bits = 0xF\nenable_panoramic = 1\n");
        let cfg = Config::load(&p);
        assert_eq!(cfg.aod_support_bits, BIT_PANORAMIC);
        let _ = fs::remove_file(p);
    }
}
