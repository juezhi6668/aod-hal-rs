# 全屏 AOD / 灵感接续 HAL —— Rust 实现

`vendor.oplus.hardware.displaypanelfeature` 的 Rust 版 AIDL HAL，用来替代 ROM 里
`/odm/bin/hw/vendor.oplus.hardware.displaypanelfeature-service`。

**已在 WSL 里交叉编译通过**，13 个单测全过。产物用
`build-in-wsl.sh` 生成，或到 Releases 下载现成的 aarch64 二进制。

> **说明**：本项目仅包含独立编写的实现代码，以及为互操作所必需的接口签名
> （从设备上公开的 AIDL Stub 事务码推导）。不包含任何厂商源码、二进制或
> 受版权保护的素材。接口名称与行为归属其各自权利人。仅用于个人设备的
> 研究、兼容性与移植；替换系统 HAL 前请自行评估风险并备份。

## 这个 HAL 怎么控制「灵感接续」和「全景息屏」

SystemUI 的 `SmoothTransitionController.initSmoothTransitionState()` 会调
`getDisplayPanelFeatureValue(0xd9, buf)`，然后按位判断：

| bit | SystemUI 字段 | 写出的 Settings.Secure | Aod.apk 里对应 |
|---|---|---|---|
| `0x1` | `gotoDozeWithOff`（反逻辑） | — | — |
| `0x2` | `isSupportSmoothTransition` | `oplus_keyguard_workshop_aod_support` | **灵感接续** |
| `0x4` | `isSupportPanoramicByPanelFeature` | `oplus_keyguard_panoramic_aod_support` | **全景息屏（全屏 AOD）** |

### 灵感接续走的是 bit 0x2，不是 0x4

`Aod.apk` 里 `aod_home_list_inspiration_title` 的中文就是「灵感接续」，
判定逻辑（`CommonUtils.isSupportInspirationAod`）：

```java
if (OS >= 16.1) {
    return F2.a.w() && getSecureInt("oplus_keyguard_workshop_aod_support") == 1;
} else {
    return isInspirationFeatureAod(ctx);
}
```

而 `oplus_keyguard_workshop_aod_support` 是 SystemUI
`setWorkshopStatusForApplication(isSupportSmoothTransition)` 写的，
`isSupportSmoothTransition` 正是 **0xd9 的 bit 0x2**。

所以：**开灵感接续 → bits 含 0x2；开全景息屏 → bits 含 0x4；两个都要 → 0x6。**

### AI 壁纸在哪一层（不在这个 HAL 里）

查过一遍，**AI 壁纸和这个 HAL 没有任何关系**：

- `vendor.oplus.hardware.displaypanelfeature-service` 和它的 ndk 库，
  搜 `wallpaper` / `ai` / `aigc` **命中 0 条**
- `Wallpapers.apk`（`com.oplus.wallpapers`）和 `Aod.apk`（`com.oplus.aod`），
  搜 `displaypanelfeature` / `IDisplayPanelFeature` **命中 0 条**

AI 壁纸实际在 `my_stock/priv-app/Wallpapers/Wallpapers.apk`：

```
AiWallpaperManager / ChooseAiWallpaperPanel / ai_wallpaper_generate
FINISH_AI_WALLPAPER_TASK_BROADCAST / download_ai_wallpaper_plugin
```

它生成的是壁纸，通过同一个 APK 里的 `com.oplus.wallpapers.InspirationThemeProvider`
喂给 Aod.apk 的「灵感接续」（Aod.apk 里 `InspirationThemeHolder`、
`com.oplus.wallpapers.InspirationThemeProvider` 就是这条）。

链路是：

```
AI 壁纸生成 (Wallpapers.apk)  →  InspirationThemeProvider
        →  Aod.apk 灵感接续（读 oplus_keyguard_workshop_aod_support）
        →  0xd9 bit 0x2  ← 本 HAL 唯一参与的地方
```

也就是说，HAL 侧**不需要为 AI 壁纸加任何东西**，`enable_inspiration = 1`
就已经把这条路打开了。

### 灵感接续不生效时：完整门槛清单

HAL 只是第一道门。Aod.apk 里 `H2/E.b0()` 决定要不要把「灵感接续」这张卡片加进列表，
条件是全部满足：

| # | 条件 | 在哪 | 不满足怎么办 |
|---|---|---|---|
| 1 | `oplus_keyguard_workshop_aod_support == 1` | Settings.Secure | 本 HAL 的 bit 0x2，见上 |
| 2 | `OplusBuild.getOplusOSVERSION() >= 38` | 系统属性 | 版本不够，改不了 |
| 3 | 不是 LCD AOD（`oplus.software.display.lcd_aod_mode` feature 不存在） | FeatureConfig | 换屏或改 feature |
| 4 | `os.personalization.theme.inspiration_aod.disable` 为 false | AppFeature | 去掉该 feature |
| 5 | **不在外屏设置页** | `isFromExternalScreenSettings()` | 灵感接续只在主屏 AOD 页显示 |
| 6 | 锁屏没换第三方包 | `oplus_customize_unlock_change_pkg` 需为空 / `com.android.keyguard` / `com.android.systemui` | 清掉该设置 |
| 7 | `apply_theme_type != 3` | 问 Wallpapers 的 `InspirationThemeProvider` | 取消全局主题 |
| 8 | 灵感主题 album 数据非空 | 同上 provider | 先应用一个灵感主题 |

第 5 条容易踩：**灵感接续在主屏 AOD 设置页，全景息屏在外屏设置页**，两者不在一个页面。

一键自检（双模式，电脑上用 adb、手机上直接 `sh /sdcard/debug-inspiration.sh` 都行）：

```bash
bash debug-inspiration.sh          # 全量
bash debug-inspiration.sh quick    # 只看关键几条
```

### 实测踩坑：配置文件没推 + 默认值只有 0x4

在 PMA110 / ColorOS V16.1.0 上真机跑自检，结果是：

```
oplus_keyguard_workshop_aod_support = 0     ← 灵感接续不出现
oplus_keyguard_panoramic_aod_support = 1    ← 全景息屏正常
```

服务在跑（pid 1107，`/odm/bin/hw/...`，347328 字节＝我们的二进制），
但 `/vendor/etc/displaypanelfeature.conf` 不存在。于是 `Config::default()`
生效，而早期版本的默认值是 `BIT_PANORAMIC` = `0x4` —— 正好解释了这个组合。

**已修复**：

1. `DEFAULT_AOD_SUPPORT_BITS` 改成 `BIT_INSPIRATION | BIT_PANORAMIC` = `0x6`，
   没配置文件时两个入口都开。加了回归测试 `default_enables_inspiration_and_panoramic`。
2. `Config::load()` 增加备用路径 `/data/local/tmp/displaypanelfeature.conf`，
   `adb push` 就能写，免 root 调试。查找顺序：
   `/vendor/etc/` → `/data/local/tmp/` → 内置默认值。
3. 启动时打日志说明用的是哪个配置源，没有配置文件会打 warn。

**不用重刷也能立刻修** —— SystemUI 的 `setWorkshopStatusForApplication()` 有
「当前值已经是 1 就跳过、绝不写回 0」的保护：

```bash
adb shell settings put secure oplus_keyguard_workshop_aod_support 1
adb shell am force-stop com.oplus.aod
```

重启后依然是 1。恢复出厂设置才会丢，届时新二进制的默认值 0x6 会自己顶上。


## 实现方式：纯 NDK FFI，不用 AIDL codegen

`src/ndk.rs` 直接声明 `libbinder_ndk` 的 C API（`AIBinder_Class_define` /
`AIBinder_new` / `AServiceManager_addService` / `AParcel_*`）：

- 不需要 AOSP 源码树
- 不需要 `aidl` 生成 Rust 桩
- 不需要 `libbinder_rs` / `bindgen` / `libclang`

只要有 NDK + 目标机的 `libbinder_ndk.so`、`liblog.so` 就能 `cargo build`。

## 目录

```
aidl/
  Android.bp                                      aidl_interface（只开 ndk backend）
  vendor/oplus/hardware/displaypanelfeature/
    IDisplayPanelFeature.aidl                     从 Stub/Proxy 逆向还原
service/
  Cargo.toml                                      cargo 交叉编译用
  Android.bp                                      Soong 用（rust_binary）
  displaypanelfeature.conf                        能力位 / 节点路径配置
  displaypanelfeature-service.rc                  init.rc
  manifest_oplus_displaypanelfeature_aidl.xml     VINTF fragment
  sepolicy/                                       .te + contexts
  src/main.rs                                     binder 回调 + 服务注册
  src/ndk.rs                                      libbinder_ndk / liblog FFI
  src/panel.rs                                    业务逻辑（纯 std，可单测）
  src/config.rs                                   配置解析
  src/lib.rs                                      给 cargo test 用
out/
  vendor.oplus.hardware.displaypanelfeature-service   已编译好的 aarch64 二进制
build-in-wsl.sh                                   一键构建脚本
```

## 构建

### WSL / Linux（NDK 交叉编译）

```bash
ROM_LIB64=/path/to/rom/system/lib64 bash build-in-wsl.sh
```

脚本做的事：跑单测 → 从 ROM dump 取 `libbinder_ndk.so` / `liblog.so` →
用 `aarch64-linux-android34-clang` 链接 → 输出到
`service/target/aarch64-linux-android/release/`。

`ROM_LIB64` 是**必填**（除非把两个 `.so` 预先放进 `service/target-libs/arm64-v8a/`，
或用 `LIBS_DIR` 指向已经放好的目录）。可覆盖的变量：

```bash
ANDROID_NDK=~/android-ndk-r27c \
ROM_LIB64=/path/to/rom/system/lib64 \
API=35 \
bash build-in-wsl.sh
```

**关键**：`libbinder_ndk.so` 必须用目标 ROM 里的那份。NDK 自带的是 stub，
只有 107 个导出符号，**没有 `AServiceManager_addService`**，链不过去。
用 ROM 的那份（本例 214 个符号）才行。

编译产物不进 git（见 `.gitignore`），发布时走 GitHub Release。

### Soong（有 AOSP 树时）

```bash
cp -r aidl    vendor/oplus/displaypanelfeature/aidl
cp -r service vendor/oplus/displaypanelfeature/service
# device.mk: PRODUCT_PACKAGES += vendor.oplus.hardware.displaypanelfeature-service
m vendor.oplus.hardware.displaypanelfeature-service
```

## 验证

```bash
adb shell service list | grep displaypanelfeature
adb logcat -s DispPanelFeature
# 期望：starting, config = ... / registered ... / joining thread pool

adb shell settings get secure oplus_keyguard_workshop_aod_support
adb shell settings get secure oplus_keyguard_panoramic_aod_support
# 重启后应为 1
```

## 配置：用友好开关，别手算掩码

```ini
enable_inspiration        = 1   # 灵感接续      → bit 0x2
enable_panoramic          = 1   # 全景息屏      → bit 0x4
enable_no_doze_with_off   = 0   # 屏灭不渐暗    → bit 0x1
```

只要出现任意一个 `enable_*`，就会接管位掩码；都没写才用原始的
`aod_support_bits = 0x6`。

**A. 只要入口出现（目标机没有 oplus_display 节点）**

```ini
enable_inspiration = 1
enable_panoramic   = 1
strict = 0
```
节点写不进去只打 warn，照样返回 0，UI 完全正常。

**B. 节点齐全，要真生效**

```ini
enable_panoramic = 1
aod_light_mode_node = /sys/kernel/oplus_display/aod_light_mode_set
aod_area_node       = /sys/kernel/oplus_display/aod_area
strict = 1
```

## 逆向过程中确认的几个坑

- **灵感接续的能力位是 `0x2`，别和全景的 `0x4` 搞混**。
  SystemUI 把 bit0x2 叫 `isSupportSmoothTransition`，实际用途是 workshop AOD，
  写出的 `oplus_keyguard_workshop_aod_support` 才是 Aod.apk 判断「灵感接续」的依据。

- **`values` 是 `out int[]`，不是 `inout`**。
  `$Stub$Proxy.getDisplayPanelFeatureValue()` 只写了 length（null 时 -1），
  没写元素；Stub 侧也是 `new int[len]` 后直接回调。写成 `inout` 会导致
  parcel 里多出/缺少元素，客户端拿到错位数据。

- **事务码固定 1/2/3**，按声明顺序分配。另外 `getInterfaceVersion` = `0xffffff`、
  `getInterfaceHash` = `0xfffffe` 也要处理，否则客户端拿版本号会失败。

- **reply 必须先写 status header**（Java 侧 `readException()` 要读它），
  再写返回值、再写 out 参数。少写这个头，客户端会把返回值当异常码。

- **服务名不能改**：`vendor.oplus.hardware.displaypanelfeature.IDisplayPanelFeature/default`，
  框架里硬编码的，名字不对 `getAidlService()` 返回 null，直接掉到 HIDL fallback。

- **只实现了 AOD 三个 feature**。其它 feature（指纹高亮等）返回 0 + 填 0，
  不会让调用方拿到空数组，但功能是空的。目标机需要别的 feature 得自己补分支。

- **`getInterfaceHash` 回的是空串**。Java 侧 `asInterface` 不校验 hash，
  C++ 侧如果校验会失败。目前已知调用方都是 Java。

## 不想要 Rust 的话

`src/panel.rs` 和 `src/config.rs` 没有任何 binder 依赖，纯 std。
换 C++ 只需照抄逻辑，胶水层用 `BnDisplayPanelFeature`（NDK backend），
跟 `~/aidlkit` 里 light HAL 的构建方式完全一样。


---

## 顺带补上的：`vendor.oplus.hardware.cwb.ICwbService`

本 ROM 的 `/system_ext/bin/theia_screen_monitor` 会以 **约 3.5 次/秒** 的频率请求
`vendor.oplus.hardware.cwb.ICwbService/default`，而这个 HAL 不存在，servicemanager
只能反复回：

```
Tried to start aidl service vendor.oplus.hardware.cwb.ICwbService/default
as a lazy service, but was unable to.
```

十分钟能刷两千多条日志。

查原机发现：**cwb 服务本来就内嵌在 `displaypanelfeature-service` 里**（同一个进程
注册两个接口），所以这里照同样做法，在 `src/cwb.rs` 里注册第二个服务。

实现范围刻意收窄：

| 方法 | 处理 |
|---|---|
| `getInterfaceVersion` | 返回 2 |
| `getInterfaceHash` | 返回空串 |
| 其余 14 个 | `STATUS_UNKNOWN_TRANSACTION` |

原因：cwb 的 AIDL 牵涉 `Rect` / `Error` / `Rgb` / `Histogram` 四个自定义 parcelable
和共享内存（`ScopedFileDescriptor`），而本 ROM 没有它的 Java stub，parcelable 布局
无法可靠推导。硬猜布局只会让客户端拿到半截数据直接崩，不如明确报「不支持」。

### 效果

```
重试日志：2149 条 → 1 条（残留的是服务启动前的那条）
客户端：TheiaScreenMonitor: got CwbService_aidl
异常：无 UNKNOWN_TRANSACTION 报错
```

theia 拿到服务后继续往下走，现在停在 `Waiting for OCenterService`（那是系统服务，
不是 HAL，与本项目无关）。

### 注意

替换 `/odm/bin/hw/vendor.oplus.hardware.displaypanelfeature-service` 后**必须重新
`chcon`**，否则 `cp` 会把 SELinux 标签冲成 `vendor_file`，init 直接拒绝启动：

```bash
chcon u:object_r:hal_sensors_default_exec:s0 \
    /vendor/odm/bin/hw/vendor.oplus.hardware.displaypanelfeature-service
```

（本 ROM 策略里没有 `oplus_hal_displaypanelfeature_exec` 这个类型，只能用
`hal_sensors_default_exec` 兜底；Permissive 下功能不受影响。）
