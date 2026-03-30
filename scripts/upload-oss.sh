#!/usr/bin/env bash
# TabPilot 产物上传到 OSS
#
# 用法: bash scripts/upload-oss.sh [build_dir]
# 需要: aliyun oss cli 或 ossutil64

set -e

PROJ_DIR="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_DIR="${1:-$PROJ_DIR/build}"
APP_NAME="TabPilot"

# OSS 配置 (使用后端同一个 bucket)
OSS_BUCKET="crafto"
OSS_ENDPOINT="oss-cn-beijing.aliyuncs.com"
OSS_CDN_DOMAIN="lingostatic.tweet.net.cn"
OSS_ACCESS_KEY_ID="${OSS_ACCESS_KEY_ID:-LTAI5tFccBPjtugyzinBGAza}"
OSS_ACCESS_KEY_SECRET="${OSS_ACCESS_KEY_SECRET:-lZ2x7YUre0JfAjDuFGWiTthmFsHYzC}"

# OSS 上传路径前缀
OSS_PREFIX="tabpilot/releases"

echo ""
echo "📦 上传 TabPilot 产物到 OSS"
echo "=========================="
echo "   Bucket: ${OSS_BUCKET}"
echo "   CDN:    https://${OSS_CDN_DOMAIN}/${OSS_PREFIX}/"
echo ""

if [ ! -d "$BUILD_DIR" ]; then
    echo "❌ 构建目录不存在: $BUILD_DIR"
    exit 1
fi

# 版本号 (从 Cargo.toml 读取)
VERSION=$(grep '^version' "$PROJ_DIR/src-tauri/Cargo.toml" | head -1 | sed 's/.*"\(.*\)"/\1/')
echo "   版本:   v${VERSION}"
echo ""

# 检测上传工具
if command -v ossutil64 &>/dev/null; then
    UPLOAD_CMD="ossutil64"
elif command -v ossutil &>/dev/null; then
    UPLOAD_CMD="ossutil"
elif command -v aliyun &>/dev/null; then
    UPLOAD_CMD="aliyun_cli"
else
    # 使用 Python oss2 (后端已安装)
    UPLOAD_CMD="python"
fi

echo "   工具:   $UPLOAD_CMD"
echo ""

upload_file() {
    local local_path="$1"
    local oss_key="$2"
    local filename=$(basename "$local_path")
    local size=$(ls -lh "$local_path" | awk '{print $5}')

    echo -n "   ⬆️  $filename ($size) → $oss_key ... "

    case "$UPLOAD_CMD" in
        ossutil64|ossutil)
            $UPLOAD_CMD cp "$local_path" "oss://${OSS_BUCKET}/${oss_key}" \
                -e "https://${OSS_ENDPOINT}" \
                -i "$OSS_ACCESS_KEY_ID" \
                -k "$OSS_ACCESS_KEY_SECRET" \
                --force >/dev/null 2>&1
            ;;
        python)
            python3 -c "
import oss2
auth = oss2.Auth('${OSS_ACCESS_KEY_ID}', '${OSS_ACCESS_KEY_SECRET}')
bucket = oss2.Bucket(auth, 'https://${OSS_ENDPOINT}', '${OSS_BUCKET}')
bucket.put_object_from_file('${oss_key}', '${local_path}')
" 2>/dev/null
            ;;
    esac

    local cdn_url="https://${OSS_CDN_DOMAIN}/${oss_key}"
    echo "✅"
    echo "         → $cdn_url"
}

# 上传 .dmg
DMG_FILE=$(find "$BUILD_DIR" -name "*.dmg" -type f | head -1)
if [ -n "$DMG_FILE" ]; then
    upload_file "$DMG_FILE" "${OSS_PREFIX}/v${VERSION}/$(basename "$DMG_FILE")"
fi

# 生成 Tauri updater JSON (darwin-aarch64.json)
# 签名文件由 cargo tauri build 自动生成
DMG_NAME=$(basename "${DMG_FILE:-TabPilot.dmg}")
DMG_SIG=""

# 查找 .sig 签名文件 (Tauri 2.x 生成在 bundle 目录)
BUNDLE_DIR="$PROJ_DIR/src-tauri/target/release/bundle"
SIG_FILE=$(find "$BUNDLE_DIR" -name "*.dmg.sig" -type f 2>/dev/null | head -1)
if [ -z "$SIG_FILE" ]; then
    # 也检查 build 目录
    SIG_FILE=$(find "$BUILD_DIR" -name "*.sig" -type f 2>/dev/null | head -1)
fi

if [ -n "$SIG_FILE" ] && [ -f "$SIG_FILE" ]; then
    DMG_SIG=$(cat "$SIG_FILE")
    echo "   🔑 签名文件: $(basename "$SIG_FILE")"
else
    echo "   ⚠️  未找到 .sig 签名文件, signature 将为空"
fi

# 写入 updater JSON
cat > "$BUILD_DIR/darwin-aarch64.json" <<EOF
{
  "version": "${VERSION}",
  "notes": "TabPilot v${VERSION}",
  "pub_date": "$(date -u +%Y-%m-%dT%H:%M:%S.000Z)",
  "url": "https://${OSS_CDN_DOMAIN}/${OSS_PREFIX}/v${VERSION}/${DMG_NAME}",
  "signature": "${DMG_SIG}"
}
EOF

upload_file "$BUILD_DIR/darwin-aarch64.json" "${OSS_PREFIX}/darwin-aarch64.json"

# 上传 latest.yml (自动更新元数据)
DMG_SIZE=$(stat -f%z "$DMG_FILE" 2>/dev/null || echo "0")
DMG_SHA512=$(shasum -a 512 "$DMG_FILE" 2>/dev/null | awk '{print $1}' || echo "")

cat > "$BUILD_DIR/latest-mac.yml" <<EOF
version: ${VERSION}
files:
  - url: ${DMG_NAME}
    sha512: ${DMG_SHA512}
    size: ${DMG_SIZE}
path: ${DMG_NAME}
sha512: ${DMG_SHA512}
releaseDate: $(date -u +%Y-%m-%dT%H:%M:%S.000Z)
EOF

upload_file "$BUILD_DIR/latest-mac.yml" "${OSS_PREFIX}/latest-mac.yml"

# 同时上传 .app 的 zip (给 Sparkle 用)
if [ -d "$BUILD_DIR/${APP_NAME}.app" ]; then
    echo -n "   📦 压缩 ${APP_NAME}.app → ${APP_NAME}-${VERSION}-mac.zip ... "
    ZIP_FILE="$BUILD_DIR/${APP_NAME}-${VERSION}-mac.zip"
    cd "$BUILD_DIR" && zip -qr "$ZIP_FILE" "${APP_NAME}.app"
    echo "✅"
    upload_file "$ZIP_FILE" "${OSS_PREFIX}/v${VERSION}/$(basename "$ZIP_FILE")"
fi

echo ""
echo "=========================="
echo "✅ 上传完成！"
echo ""
echo "下载地址:"
echo "   DMG: https://${OSS_CDN_DOMAIN}/${OSS_PREFIX}/v${VERSION}/${DMG_NAME}"
echo "   ZIP: https://${OSS_CDN_DOMAIN}/${OSS_PREFIX}/v${VERSION}/${APP_NAME}-${VERSION}-mac.zip"
echo ""
