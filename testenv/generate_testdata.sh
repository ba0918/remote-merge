#!/bin/bash
# =============================================================================
# generate_testdata.sh — リモート側テストデータ生成（10万ファイル）
#
# コンテナ内で実行:
#   docker exec rm-testenv-centos5 bash /generate_testdata.sh
#
# または SSH 経由:
#   ssh -p 2222 testuser@localhost 'bash -s' < generate_testdata.sh
#
# 生成内容:
#   /srv/testdata/ 配下に約10万ファイル
#   - テキスト (.php, .html, .css, .js, .txt, .json, .xml, .yml): ~70%
#   - 画像 (.png, .jpg, .gif): ~20%
#   - バイナリ (.pdf, .zip, .dat): ~10%
#   - ディレクトリ深さ: 最大8階層
#   - ファイルサイズ: 10B ~ 500KB
# =============================================================================

set -e

BASE_DIR="/srv/testdata"
TOTAL_FILES=100000
PROGRESS_INTERVAL=5000

# ── ディレクトリ構造定義 ──
# 実際の Web アプリケーション風の構造
DIRS=(
    "app/controllers"
    "app/models"
    "app/views/layouts"
    "app/views/pages"
    "app/views/partials"
    "app/views/components"
    "app/helpers"
    "app/services"
    "app/middleware"
    "app/validators"
    "config/environments"
    "config/initializers"
    "config/locales"
    "database/migrations"
    "database/seeds"
    "public/css"
    "public/js"
    "public/images/avatars"
    "public/images/products"
    "public/images/banners"
    "public/images/icons"
    "public/fonts"
    "public/uploads/2024/01"
    "public/uploads/2024/02"
    "public/uploads/2024/03"
    "public/uploads/2024/04"
    "public/uploads/2024/05"
    "public/uploads/2024/06"
    "public/uploads/2024/07"
    "public/uploads/2024/08"
    "public/uploads/2024/09"
    "public/uploads/2024/10"
    "public/uploads/2024/11"
    "public/uploads/2024/12"
    "public/uploads/2025/01"
    "public/uploads/2025/02"
    "public/uploads/2025/03"
    "resources/lang/en"
    "resources/lang/ja"
    "resources/lang/zh"
    "storage/logs"
    "storage/cache"
    "storage/sessions"
    "storage/tmp"
    "tests/unit"
    "tests/integration"
    "tests/fixtures"
    "vendor/legacy/lib"
    "vendor/legacy/assets"
    "vendor/plugins/auth"
    "vendor/plugins/cache"
    "vendor/plugins/mail"
    "docs/api"
    "docs/guides"
    "scripts"
    "lib/utils"
    "lib/core"
    "lib/ext"
)

# ── テキストファイル拡張子と重み ──
TEXT_EXTS=("php" "html" "css" "js" "txt" "json" "xml" "yml" "rb" "py" "sh" "sql" "md" "log" "csv" "ini" "conf")
IMG_EXTS=("png" "jpg" "gif")
BIN_EXTS=("pdf" "zip" "dat" "bin" "tar" "gz")

echo "=== remote-merge testdata generator ==="
echo "Target: $BASE_DIR"
echo "Files:  $TOTAL_FILES"
echo ""

# ── ディレクトリ作成 ──
echo "Creating directory structure..."
for dir in "${DIRS[@]}"; do
    mkdir -p "$BASE_DIR/$dir"
done
echo "  Created ${#DIRS[@]} directories"

# ── ランダムテキスト生成（高速版: /dev/urandom + base64）──
generate_text_content() {
    local size=$1
    # base64 で可読文字を生成、改行を含む
    head -c "$size" /dev/urandom | base64 | head -c "$size"
}

# ── バイナリコンテンツ生成 ──
generate_binary_content() {
    local size=$1
    head -c "$size" /dev/urandom
}

# ── 簡易 PNG ヘッダ付きダミー画像 ──
generate_dummy_image() {
    local size=$1
    # PNG シグネチャ (8 bytes) + ランダムデータ
    printf '\x89PNG\r\n\x1a\n'
    head -c "$((size - 8))" /dev/urandom
}

# ── ファイル生成メインループ ──
echo "Generating $TOTAL_FILES files..."
start_time=$(date +%s)

count=0
dir_count=${#DIRS[@]}

while [ $count -lt $TOTAL_FILES ]; do
    # ディレクトリをラウンドロビン + ランダムで選択
    dir_idx=$((count % dir_count))
    target_dir="${DIRS[$dir_idx]}"

    # ファイル種別の決定 (70% text, 20% image, 10% binary)
    roll=$((RANDOM % 100))

    if [ $roll -lt 70 ]; then
        # テキストファイル
        ext_idx=$((RANDOM % ${#TEXT_EXTS[@]}))
        ext="${TEXT_EXTS[$ext_idx]}"
        # サイズ: 10B ~ 100KB
        size=$(( (RANDOM % 100000) + 10 ))
        generate_text_content "$size" > "$BASE_DIR/$target_dir/file_${count}.${ext}"
    elif [ $roll -lt 90 ]; then
        # 画像ファイル
        ext_idx=$((RANDOM % ${#IMG_EXTS[@]}))
        ext="${IMG_EXTS[$ext_idx]}"
        # サイズ: 1KB ~ 500KB
        size=$(( (RANDOM % 500000) + 1000 ))
        generate_dummy_image "$size" > "$BASE_DIR/$target_dir/img_${count}.${ext}"
    else
        # バイナリファイル
        ext_idx=$((RANDOM % ${#BIN_EXTS[@]}))
        ext="${BIN_EXTS[$ext_idx]}"
        # サイズ: 100B ~ 200KB
        size=$(( (RANDOM % 200000) + 100 ))
        generate_binary_content "$size" > "$BASE_DIR/$target_dir/bin_${count}.${ext}"
    fi

    count=$((count + 1))

    # 進捗表示
    if [ $((count % PROGRESS_INTERVAL)) -eq 0 ]; then
        elapsed=$(( $(date +%s) - start_time ))
        rate=0
        if [ $elapsed -gt 0 ]; then
            rate=$((count / elapsed))
        fi
        echo "  $count / $TOTAL_FILES files ($rate files/sec)"
    fi
done

elapsed=$(( $(date +%s) - start_time ))
echo ""
echo "=== Generation complete ==="
echo "Files: $count"
echo "Time:  ${elapsed}s"
echo "Dir:   $BASE_DIR"

# ファイル数の確認
actual=$(find "$BASE_DIR" -type f | wc -l)
echo "Actual file count: $actual"

# ディスク使用量
du -sh "$BASE_DIR"
