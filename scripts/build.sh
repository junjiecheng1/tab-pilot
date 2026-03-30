#!/usr/bin/env bash
# TabPilot 一键构建 + 安装
#
# Rust 自动选择环境:
#   cargo tauri dev   → 加载 .env (开发)
#   cargo tauri build → 加载 .env.production (生产)
#
# 用法:
#   bash scripts/build.sh           # 生产环境构建
#   bash scripts/build.sh --dev     # 开发环境构建 (用 .env)
#   SKIP_UPLOAD=true bash scripts/build.sh  # 构建 + 安装, 不上传

set -e

PROJ_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="$PROJ_DIR/build"
BUNDLE_DIR="$PROJ_DIR/src-tauri/target/release/bundle"
APP_NAME="TabPilot"

# 解析参数
DEV_MODE=false
if [ "$1" = "--dev" ]; then
  DEV_MODE=true
fi

ENV_MODE="production"
if [ "$DEV_MODE" = true ]; then
  ENV_MODE="development"
fi

echo "🔨 TabPilot Build & Install"
echo "=========================="
echo "   环境: $ENV_MODE"

# dev 模式: 导出 .env 变量 (覆盖 .env.production 的同名变量)
if [ "$DEV_MODE" = true ]; then
  set -a
  source "$PROJ_DIR/.env"
  set +a
  echo "   已加载 .env 环境变量"
fi

# 1. 清理旧产物
echo ""
echo "① 清理旧产物..."
rm -rf "$PROJ_DIR/dist"
rm -rf "$BUILD_DIR"
mkdir -p "$BUILD_DIR"

# 2. 构建 (前端 + Rust + Bundle)
echo ""
echo "② 构建中 (前端 → Rust → Bundle)..."
cd "$PROJ_DIR"
TAURI_SIGNING_PRIVATE_KEY="${TAURI_SIGNING_PRIVATE_KEY:-$(cat ~/.tauri/tabpilot.key 2>/dev/null || echo '')}" \
  cargo tauri build --bundles app,dmg 2>&1

# 4. 收集产物到 build/
echo ""
echo "③ 收集产物..."
if [ -d "$BUNDLE_DIR/macos/${APP_NAME}.app" ]; then
    cp -R "$BUNDLE_DIR/macos/${APP_NAME}.app" "$BUILD_DIR/"
    echo "   ✅ ${APP_NAME}.app"
fi
if ls "$BUNDLE_DIR/dmg/"*.dmg 1>/dev/null 2>&1; then
    cp "$BUNDLE_DIR/dmg/"*.dmg "$BUILD_DIR/"
    echo "   ✅ $(ls "$BUNDLE_DIR/dmg/"*.dmg | xargs basename)"
fi

# 5. 安装到 /Applications
echo ""
echo "④ 安装到 /Applications..."
pkill -f "/Applications/${APP_NAME}.app" 2>/dev/null || true
sleep 1
rm -rf "/Applications/${APP_NAME}.app"
cp -R "$BUILD_DIR/${APP_NAME}.app" "/Applications/"
echo "   ✅ 已安装"

# 6. 注册 URL scheme
echo ""
echo "⑤ 注册 tabpilot:// URL scheme..."
/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f "/Applications/${APP_NAME}.app"
echo "   ✅ 已注册"

# 7. 上传到 OSS
echo ""
echo "⑥ 上传到 OSS..."
if [ "$SKIP_UPLOAD" != "true" ]; then
    bash "$PROJ_DIR/scripts/upload-oss.sh" "$BUILD_DIR"
else
    echo "   ⏭️  跳过 (SKIP_UPLOAD=true)"
fi

# 8. 启动
echo ""
echo "⑦ 启动 ${APP_NAME}..."
open "/Applications/${APP_NAME}.app"
echo "   ✅ 已启动"

# 9. 输出信息
echo ""
echo "=========================="
echo "✅ 构建完成！ (环境: $ENV_MODE)"
echo ""
ls -lh "$BUILD_DIR/"
