#!/usr/bin/env bash
# 在 WSL 里用 NDK 交叉编译出真机可用的 aarch64 二进制。
#
#   ROM_LIB64=/path/to/rom/system/lib64 bash build-in-wsl.sh
#
# ROM_LIB64 指向目标 ROM 解包后的 system/lib64，里面必须有
# libbinder_ndk.so 和 liblog.so。也可以用 LIBS_DIR 直接指定一个
# 已经放好这两个 .so 的目录，跳过拷贝。
#
set -euo pipefail
cd "$(dirname "$0")/service"

NDK="${ANDROID_NDK:-$HOME/android-ndk-r26d}"
TC="$NDK/toolchains/llvm/prebuilt/linux-x86_64"
API="${API:-34}"
TARGET="aarch64-linux-android"
LIBS="${LIBS_DIR:-$PWD/target-libs/arm64-v8a}"
ROM="${ROM_LIB64:-}"
OUT="vendor.oplus.hardware.displaypanelfeature-service"

echo "== toolchain =="
rustc --version
"$TC/bin/${TARGET}${API}-clang" --version | head -1

echo
echo "== 单元测试（纯逻辑）=="
cargo test --quiet

echo
echo "== 准备目标库 =="
mkdir -p "$LIBS"
for f in libbinder_ndk.so liblog.so; do
    if [ -f "$LIBS/$f" ]; then
        continue
    fi
    if [ -z "$ROM" ] || [ ! -f "$ROM/$f" ]; then
        cat >&2 <<EOF
缺少 $f。

请用下面任一方式提供目标 ROM 的库（不能用 NDK 自带的 stub，它没有
AServiceManager_addService）：

  1) ROM_LIB64=/path/to/rom/system/lib64 bash build-in-wsl.sh
  2) 把 libbinder_ndk.so / liblog.so 放进 $LIBS 后重跑
  3) LIBS_DIR=/path/to/your/libs bash build-in-wsl.sh
EOF
        exit 1
    fi
    cp -f "$ROM/$f" "$LIBS/$f"
done
ls -la "$LIBS"

echo
echo "== 交叉编译 =="
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/bin/${TARGET}${API}-clang"
export RUSTFLAGS="-L native=$LIBS"
cargo build --release --features hal --target "$TARGET"

BIN="target/$TARGET/release/displaypanelfeature_service"
FINAL="target/$TARGET/release/$OUT"
cp -f "$BIN" "$FINAL"
echo
echo "== 产物 =="
file "$FINAL"
ls -la "$FINAL"

echo
echo "== 依赖检查 =="
"$TC/bin/llvm-readelf" -d "$FINAL" | grep NEEDED
echo "-- 确认 binder 符号是从真机 lib 解析的 --"
"$TC/bin/llvm-nm" -D --undefined-only "$FINAL" | grep -E "AServiceManager_addService|AIBinder_Class_define" || true
