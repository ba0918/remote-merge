#!/bin/bash
# =============================================================================
# generate_local_data.sh — ローカル側テストデータ生成（乖離シナリオ）
#
# リモートの10万ファイルに対して、ローカルには一部のみ存在する状態を作る。
# これにより大量の RightOnly（リモートにしかない）ファイルが発生する。
#
# 使い方:
#   ./generate_local_data.sh
#
# シナリオ:
#   - ローカルにある: 500 ファイル（リモートと共通のパスの一部）
#   - うち Modified:  200 ファイル（内容が異なる）
#   - うち Equal:     200 ファイル（内容が同一）
#   - LeftOnly:       100 ファイル（ローカルにしかない）
#   - RightOnly:    99500 ファイル（リモートにしかない）
# =============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
LOCAL_DIR="$SCRIPT_DIR/data/local"
REMOTE_DIR="/srv/testdata"  # リモート（コンテナ内）のパス

# コンテナ名
CONTAINER="rm-testenv-centos5"

echo "=== Local test data generator ==="
echo "Target: $LOCAL_DIR"
echo ""

# ── クリーンアップ ──
rm -rf "$LOCAL_DIR"
mkdir -p "$LOCAL_DIR"

# ── リモートからファイル一覧を取得 ──
echo "Fetching remote file list from container..."
REMOTE_FILES=$(docker exec "$CONTAINER" find "$REMOTE_DIR" -type f | head -400)

if [ -z "$REMOTE_FILES" ]; then
    echo "ERROR: No remote files found. Run generate_testdata.sh first."
    exit 1
fi

file_count=0

# ── Equal ファイル (200): リモートと同一内容をコピー ──
echo "Creating Equal files (200)..."
equal_count=0
while IFS= read -r remote_path; do
    [ $equal_count -ge 200 ] && break

    # リモートパスからローカルパスを算出
    rel_path="${remote_path#$REMOTE_DIR/}"
    local_path="$LOCAL_DIR/$rel_path"

    # ディレクトリ作成
    mkdir -p "$(dirname "$local_path")"

    # リモートからファイル内容をコピー
    docker cp "$CONTAINER:$remote_path" "$local_path" 2>/dev/null || continue

    equal_count=$((equal_count + 1))
    file_count=$((file_count + 1))
done <<< "$REMOTE_FILES"
echo "  Created $equal_count Equal files"

# ── Modified ファイル (200): リモートと同じパスだが内容が異なる ──
echo "Creating Modified files (200)..."
modified_count=0
REMAINING_FILES=$(echo "$REMOTE_FILES" | tail -n +201)
while IFS= read -r remote_path; do
    [ $modified_count -ge 200 ] && break

    rel_path="${remote_path#$REMOTE_DIR/}"
    local_path="$LOCAL_DIR/$rel_path"

    mkdir -p "$(dirname "$local_path")"

    # 異なる内容を生成
    echo "// Modified locally at $(date)" > "$local_path"
    echo "// Original path: $rel_path" >> "$local_path"
    head -c $((RANDOM % 5000 + 100)) /dev/urandom | base64 >> "$local_path"

    modified_count=$((modified_count + 1))
    file_count=$((file_count + 1))
done <<< "$REMAINING_FILES"
echo "  Created $modified_count Modified files"

# ── LeftOnly ファイル (100): ローカルにしかない ──
echo "Creating LeftOnly files (100)..."
leftonly_dirs=(
    "app/local_only"
    "config/local"
    "scripts/deploy"
    "tests/local"
    "docs/internal"
)
for dir in "${leftonly_dirs[@]}"; do
    mkdir -p "$LOCAL_DIR/$dir"
done

for i in $(seq 1 100); do
    dir_idx=$((i % ${#leftonly_dirs[@]}))
    dir="${leftonly_dirs[$dir_idx]}"
    echo "// Local only file #$i" > "$LOCAL_DIR/$dir/local_${i}.txt"
    echo "Created: $(date)" >> "$LOCAL_DIR/$dir/local_${i}.txt"
    head -c $((RANDOM % 2000 + 50)) /dev/urandom | base64 >> "$LOCAL_DIR/$dir/local_${i}.txt"
    file_count=$((file_count + 1))
done
echo "  Created 100 LeftOnly files"

echo ""
echo "=== Generation complete ==="
echo "Total local files: $file_count"
echo "  Equal:    $equal_count"
echo "  Modified: $modified_count"
echo "  LeftOnly: 100"
echo "  RightOnly (remote): ~$((100000 - equal_count - modified_count))"
echo ""
echo "Dir: $LOCAL_DIR"
find "$LOCAL_DIR" -type f | wc -l
du -sh "$LOCAL_DIR"
