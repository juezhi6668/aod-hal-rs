#!/usr/bin/env sh
# 灵感接续不生效时的逐层自检（双模式）。
#
#   在电脑上（有 adb）：   bash debug-inspiration.sh
#   在手机上（终端 App）： sh /sdcard/debug-inspiration.sh
#   只看关键几条：          sh /sdcard/debug-inspiration.sh quick
#
# 脚本会自动判断自己在哪一侧运行，不需要 adb 也能跑。
# 涉及 /vendor /odm 的检查需要 root；没有 root 会标注为 SKIP。

set -u

QUICK="${1:-}"

if command -v adb >/dev/null 2>&1 && adb get-state >/dev/null 2>&1; then
    MODE=pc
else
    MODE=device
fi

# 统一执行入口：传一条完整的命令字符串
RUN() {
    if [ "$MODE" = pc ]; then
        adb shell "$1" 2>/dev/null
    else
        sh -c "$1" 2>/dev/null
    fi
}

# root 命令；没 root 就返回空
ROOT() {
    if [ "$MODE" = pc ]; then
        adb shell su -c "$1" 2>/dev/null
    else
        if command -v su >/dev/null 2>&1; then
            su -c "$1" 2>/dev/null
        else
            return 1
        fi
    fi
}

HAS_ROOT=0
ROOT "id" | grep -q "uid=0" && HAS_ROOT=1

pass() { printf '  [OK]   %s\n' "$*"; }
fail() { printf '  [FAIL] %s\n' "$*"; }
warn() { printf '  [WARN] %s\n' "$*"; }
info() { printf '         %s\n' "$*"; }
hdr()  { printf '\n== %s ==\n' "$*"; }

printf '运行模式: %s   root: %s\n' "$MODE" "$([ "$HAS_ROOT" = 1 ] && echo yes || echo no)"

# ---------------------------------------------------------------- 1. 服务
hdr "1. HAL 服务是否注册"
SVCLIST=$(RUN "service list")
if echo "$SVCLIST" | grep -q "displaypanelfeature"; then
    pass "service list 里有 displaypanelfeature"
    echo "$SVCLIST" | grep "displaypanelfeature" | sed 's/^/         /'
else
    fail "service list 里没有 displaypanelfeature"
    info "注意：有些 ROM 的 service list 不列 vendor 服务，以第 1b/1c 节为准"
fi

hdr "1b. 进程在不在"
PS=$(RUN "ps -A")
if echo "$PS" | grep -q "displaypanelfeature-service"; then
    pass "进程在跑"
    echo "$PS" | grep "displaypanelfeature-service" | sed 's/^/         /'
else
    fail "进程不在 —— 可能 addService 撞名后退出，或 init.rc 没生效"
fi

hdr "1c. HAL 自己的日志（最关键）"
if [ "$MODE" = pc ]; then
    LOGS=$(adb logcat -d -s DispPanelFeature 2>/dev/null | tail -20)
else
    LOGS=$(logcat -d -s DispPanelFeature 2>/dev/null | tail -20)
fi
if [ -z "$LOGS" ]; then
    fail "一条 DispPanelFeature 日志都没有 —— 服务从没启动过"
else
    echo "$LOGS" | sed 's/^/         /'
    if echo "$LOGS" | grep -q "add_service failed"; then
        fail "addService 失败 —— 实例名被原厂服务占了"
    elif echo "$LOGS" | grep -q "registered"; then
        pass "注册成功"
    fi
    if echo "$LOGS" | grep -q "get 0xd9"; then
        pass "SystemUI 已经在向我们查 0xd9"
        echo "$LOGS" | grep "get 0xd9" | tail -3 | sed 's/^/         /'
    else
        warn "没有 get 0xd9 调用 —— SystemUI 没走到我们这里"
        info "重启 SystemUI 后再抓一次日志"
    fi
fi

# ---------------------------------------------------------------- 2. settings
hdr "2. SystemUI 写出的能力标志"
for k in oplus_keyguard_workshop_aod_support \
         oplus_keyguard_panoramic_aod_support; do
    v=$(RUN "settings get secure $k")
    printf '         %-45s = %s\n' "$k" "${v:-<null>}"
done

if [ "$QUICK" = "quick" ]; then
    hdr "quick 模式结束"
    exit 0
fi

WS=$(RUN "settings get secure oplus_keyguard_workshop_aod_support")
[ "$WS" = "1" ] && pass "灵感接续能力位 = 1" || fail "灵感接续能力位 = ${WS:-null}，期望 1"
PAN=$(RUN "settings get secure oplus_keyguard_panoramic_aod_support")
[ "$PAN" = "1" ] && pass "全景息屏能力位 = 1" || fail "全景息屏能力位 = ${PAN:-null}，期望 1"

# ---------------------------------------------------------------- 3. 前置条件
hdr "3. Aod.apk 里的硬性前置条件"

OSV=$(RUN "getprop ro.build.version.oplusrom")
[ -z "$OSV" ] && OSV=$(RUN "getprop ro.oplus.version")
OSV2=$(RUN "getprop ro.build.version.oplus")
info "ro.build.version.oplusrom = ${OSV:-<空>}   /   ro.build.version.oplus = ${OSV2:-<空>}"
info "要求 OplusBuild.getOplusOSVERSION() >= 38（ColorOS 16.1）"

LCD=$(RUN "getprop ro.oplus.display.lcd_aod_mode")
if [ -n "$LCD" ] && [ "$LCD" != "0" ]; then
    fail "lcd_aod_mode 开着 —— isInspirationFeatureAod 直接返回 false"
else
    pass "不是 LCD AOD 模式（${LCD:-<未设置>}）"
fi

DIS=$(RUN "settings get secure os.personalization.theme.inspiration_aod.disable")
case "$DIS" in
    ""|"null"|"0") pass "inspiration_aod.disable = ${DIS:-<未设置>}（未禁用）" ;;
    *)             fail "inspiration_aod.disable = $DIS —— 入口会被隐藏" ;;
esac

LEG=$(RUN "settings get secure os.personalization.theme.inspiration_aod.force_enable_legacy")
info "force_enable_legacy = ${LEG:-<未设置>}"

UPKG=$(RUN "settings get system oplus_customize_unlock_change_pkg")
case "$UPKG" in
    ""|"null"|"com.android.keyguard"|"com.android.systemui")
        pass "oplus_customize_unlock_change_pkg = ${UPKG:-<空>}（未换第三方锁屏）" ;;
    *)  fail "锁屏被换成 $UPKG —— w2/a.i() 为 false，灵感接续入口隐藏" ;;
esac

info "apply_theme_type 需 != 3（=3 表示套了全局主题）"
GT=$(RUN "content call --uri content://com.oplus.wallpapers.InspirationThemeProvider --method get_theme_model 2>&1")
[ -n "$GT" ] && echo "$GT" | head -5 | sed 's/^/         /' || info "provider 没应答（返回 0，不阻塞）"

# ---------------------------------------------------------------- 4. 安装情况
hdr "4. HAL 二进制 / 配置 / VINTF"
if [ "$HAS_ROOT" = 1 ]; then
    BIN=$(ROOT "ls -l /vendor/bin/hw/vendor.oplus.hardware.displaypanelfeature-service 2>/dev/null")
    [ -n "$BIN" ] && { pass "二进制在 /vendor/bin/hw/"; info "$BIN"; } \
                  || fail "/vendor/bin/hw/ 下没有我们的二进制"

    CONF=$(ROOT "cat /vendor/etc/displaypanelfeature.conf 2>/dev/null")
    if [ -n "$CONF" ]; then
        pass "配置文件在"
        echo "$CONF" | grep -E "enable_|aod_support_bits|strict" | sed 's/^/         /'
    else
        warn "没有 /vendor/etc/displaypanelfeature.conf，将用默认值"
    fi

    VINTF=$(ROOT "grep -rl displaypanelfeature /vendor/etc/vintf /odm/etc/vintf /system/etc/vintf 2>/dev/null")
    CNT=$(printf '%s\n' "$VINTF" | grep -c . )
    if [ "$CNT" -gt 1 ]; then
        fail "找到 $CNT 份 VINTF fragment —— 原厂和我们的同时在，必须删一份"
        printf '%s\n' "$VINTF" | sed 's/^/         /'
    else
        pass "VINTF fragment 数量 = $CNT"
    fi
else
    warn "没 root，跳过 /vendor /odm 检查（用电脑端跑或给终端 App root）"
fi

# ---------------------------------------------------------------- 5. SELinux
hdr "5. SELinux / 原厂服务"
if [ "$MODE" = pc ]; then
    AVC=$(adb logcat -d 2>/dev/null | grep -i "avc.*denied" | grep -i "displaypanelfeature" | tail -5)
else
    AVC=$(logcat -d 2>/dev/null | grep -i "avc.*denied" | grep -i "displaypanelfeature" | tail -5)
fi
if [ -n "$AVC" ]; then
    fail "有 avc denied"
    echo "$AVC" | sed 's/^/         /'
else
    pass "没有 displaypanelfeature 相关的 avc denied"
fi
info "SELinux = $(RUN 'getenforce')"

if [ "$HAS_ROOT" = 1 ]; then
    ORIG=$(ROOT "ls /odm/bin/hw/vendor.oplus.hardware.displaypanelfeature-service /vendor/bin/hw/vendor.oplus.hardware.displaypanelfeature-service 2>/dev/null")
    if [ -n "$ORIG" ]; then
        warn "发现同时存在多个实现，检查哪个是原厂："
        echo "$ORIG" | sed 's/^/         /'
        info "两份服务抢同一实例名，先注册的赢。必须禁掉原厂那份（改名二进制 + 注释 init.rc）"
    else
        pass "没有发现 odm 下的原厂二进制"
    fi
else
    warn "没 root，跳过原厂服务检查"
fi

# ---------------------------------------------------------------- 6. Wallpapers
hdr "6. 灵感主题数据源"
PKGS=$(RUN "pm list packages")
if echo "$PKGS" | grep -q "com.oplus.wallpapers"; then
    pass "com.oplus.wallpapers 已安装"
else
    fail "com.oplus.wallpapers 没装 —— InspirationThemeProvider 不存在，卡片会是空的"
fi

# ---------------------------------------------------------------- 结论
hdr "结论怎么看"
cat <<'EOF'
     1) 没有 "registered"          -> 服务没起来：查 init.rc / VINTF / avc / 原厂残留
     2) 有 "add_service failed"    -> 实例名被占，禁用原厂服务
     3) 有 registered 但没 get 0xd9 -> 重启 SystemUI
     4) 有 get 0xd9 但第 2 节是 0  -> SystemUI 逻辑被别的条件挡住
     5) 第 2 节是 1 但入口不出现    -> 查第 3 节（版本/lcd/disable/第三方锁屏/apply_theme_type）
                                      和第 6 节（provider 没数据）

     灵感接续只在「主屏 AOD 设置页」；全景息屏在「外屏设置页」。

     重启 SystemUI：
       su -c 'killall com.android.systemui'
EOF
