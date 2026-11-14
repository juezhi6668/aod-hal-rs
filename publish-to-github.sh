#!/usr/bin/env bash
# 把本项目推送到 GitHub。
#
# 前提：先在 GitHub 网页上建一个**空仓库**（不要勾 README / LICENSE / .gitignore）。
#
# 用法：
#   GITHUB_USER=yourname bash publish-to-github.sh
#
# 认证（任选其一，凭据不会写进任何文件）：
#   A. SSH   —— 已配好 ~/.ssh 公钥并加到 GitHub，什么都不用设
#   B. HTTPS —— Git Credential Manager / 系统凭据，首次会弹浏览器登录（推荐）
#   C. Token —— GITHUB_TOKEN=ghp_xxx 环境变量，仅用于 push 的这一次调用
#
# 可选：
#   REPO             仓库名，默认 aod-hal-rs
#   GIT_NAME/EMAIL   仅本仓库生效的提交者身份
#   DRY_RUN=1        只打印将要执行的命令，不真的推

set -euo pipefail
cd "$(dirname "$0")"

USER_NAME="${GITHUB_USER:-}"
REPO_NAME="${REPO:-aod-hal-rs}"
BRANCH="${BRANCH:-main}"

if [ -z "$USER_NAME" ]; then
    echo "请设置 GITHUB_USER，例如："
    echo "  GITHUB_USER=yourname bash publish-to-github.sh"
    exit 1
fi

# remote URL 永远保持干净，绝不把 token 拼进去（那样会写进 .git/config）
if [ -f "$HOME/.ssh/id_ed25519" ] || [ -f "$HOME/.ssh/id_rsa" ]; then
    REMOTE="git@github.com:${USER_NAME}/${REPO_NAME}.git"
else
    REMOTE="https://github.com/${USER_NAME}/${REPO_NAME}.git"
fi

echo "仓库: $REMOTE"
echo "分支: $BRANCH"

run() {
    echo "+ $*"
    [ "${DRY_RUN:-0}" = "1" ] || "$@"
}

# 本仓库的提交者身份（不污染全局 git config）
if [ -n "${GIT_NAME:-}" ]; then
    run git config user.name "$GIT_NAME"
fi
if [ -n "${GIT_EMAIL:-}" ]; then
    run git config user.email "$GIT_EMAIL"
fi

if [ "${DRY_RUN:-0}" != "1" ] &&
   { ! git config user.name >/dev/null 2>&1 || ! git config user.email >/dev/null 2>&1; }; then
    cat >&2 <<'EOF'
缺少 git 提交者身份。任选其一：

  GIT_NAME="Your Name" GIT_EMAIL=you@example.com \
    GITHUB_USER=yourname bash publish-to-github.sh

或一次性配置全局：
  git config --global user.name "Your Name"
  git config --global user.email "you@example.com"
EOF
    exit 1
fi

# 首次提交
if ! git rev-parse --verify HEAD >/dev/null 2>&1; then
    run git add -A
    run git commit -m "Initial import: Rust AIDL HAL for Oplus displaypanelfeature

Implements vendor.oplus.hardware.displaypanelfeature via libbinder_ndk FFI,
covering AOD light mode (0x07), AOD area (0xC0) and AOD capability bits (0xD9):
inspiration continuation and panoramic (full-screen) AOD.

Includes AIDL definition, Soong module, init.rc, VINTF manifest, SELinux
policy, standalone NDK cross-build script and an on-device diagnostic script."
fi

run git branch -M "$BRANCH"

if git remote get-url origin >/dev/null 2>&1; then
    run git remote set-url origin "$REMOTE"
else
    run git remote add origin "$REMOTE"
fi

# token 走一次性 extraheader，只在这次 push 的进程里存在，不落盘
if [ -n "${GITHUB_TOKEN:-}" ]; then
    if [ "${DRY_RUN:-0}" = "1" ]; then
        echo "+ git -c http.extraheader=<hidden> push -u origin $BRANCH"
    else
        echo "+ git push -u origin $BRANCH (token via extraheader)"
        git -c "http.extraheader=AUTHORIZATION: bearer ${GITHUB_TOKEN}" \
            push -u origin "$BRANCH"
    fi
else
    run git push -u origin "$BRANCH"
fi

echo
echo "完成：https://github.com/${USER_NAME}/${REPO_NAME}"
echo
echo "可选：把编译好的二进制发到 Release（out/ 按 .gitignore 不进仓库）"
echo "  gh release create v0.1.0 out/vendor.oplus.hardware.displaypanelfeature-service"
