#!/bin/bash
# =============================================================================
# setup.sh — テスト環境のセットアップ一式
#
# 使い方:
#   cd testenv && ./setup.sh
#
# 前提:
#   - WSL2: kernelCommandLine = vsyscall=emulate (.wslconfig)
#   - Docker Desktop or Docker Engine
#   - rpms/ に CentOS 5 用 RPM をダウンロード済み
#     (download_rpms.sh で取得可能)
#
# 実行内容:
#   1. RSA 鍵生成（なければ）
#   2. Docker イメージビルド
#   3. コンテナ起動
#   4. SSH 接続確認
#   5. リモートテストデータ生成 (10万ファイル)
#   6. ローカルテストデータ生成 (乖離シナリオ)
# =============================================================================

set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

SSH_KEY=".ssh/id_rsa"
# CentOS 5 の openssh 4.3 はレガシーアルゴリズムのみ対応
SSH_OPTS="-o StrictHostKeyChecking=no -o ConnectTimeout=5 \
  -o KexAlgorithms=+diffie-hellman-group14-sha1 \
  -o HostKeyAlgorithms=+ssh-rsa,ssh-dss \
  -o PubkeyAcceptedKeyTypes=+ssh-rsa"

echo "========================================"
echo " remote-merge test environment setup"
echo "========================================"
echo ""

# ── Step 1: RSA 鍵生成 ──
echo "[1/6] Checking SSH key..."
if [ ! -f "$SSH_KEY" ]; then
    echo "  Generating RSA key pair..."
    mkdir -p .ssh
    ssh-keygen -t rsa -b 2048 -f "$SSH_KEY" -N '' -C 'testenv'
else
    echo "  RSA key already exists"
fi
echo ""

# ── Step 2: Docker ビルド ──
echo "[2/6] Building Docker image..."
docker compose build
echo ""

# ── Step 3: コンテナ起動 ──
echo "[3/6] Starting container..."
docker compose up -d
echo "  Waiting for sshd to start..."
sleep 3
echo ""

# ── Step 4: SSH 接続確認 ──
echo "[4/6] Testing SSH connection..."
# known_hosts のエントリをリフレッシュ
ssh-keygen -f ~/.ssh/known_hosts -R '[localhost]:2222' 2>/dev/null || true

if ssh $SSH_OPTS -i "$SSH_KEY" -p 2222 testuser@localhost \
    'echo "  SSH OK: $(cat /etc/redhat-release), bash $(bash --version | head -1 | grep -o "[0-9]\+\.[0-9]\+\.[0-9]\+")"' 2>/dev/null; then
    echo "  SSH connection successful"
else
    echo "  ERROR: SSH connection failed"
    echo "  Check: docker ps, docker logs rm-testenv-centos5"
    exit 1
fi
echo ""

# ── Step 5: リモートテストデータ生成 ──
echo "[5/6] Generating remote test data (100,000 files)..."
echo "  This may take several minutes..."
docker cp generate_testdata.sh rm-testenv-centos5:/tmp/generate_testdata.sh
docker exec rm-testenv-centos5 bash /tmp/generate_testdata.sh
echo ""

# ── Step 6: ローカルテストデータ生成 ──
echo "[6/6] Generating local test data (divergence scenario)..."
bash generate_local_data.sh
echo ""

echo "========================================"
echo " Setup complete!"
echo "========================================"
echo ""
echo "Usage:"
echo "  # Single file diff (should be instant after P1 fix)"
echo "  cargo run -- --config testenv/config.toml diff --right centos5 app/controllers/file_0.php"
echo ""
echo "  # Full scan status (tests P2 chunk splitting)"
echo "  cargo run -- --config testenv/config.toml status --right centos5"
echo ""
echo "  # Full scan diff (tests P2 + P4)"
echo "  cargo run -- --config testenv/config.toml diff --right centos5 ."
echo ""
echo "Teardown:"
echo "  cd testenv && docker compose down"
echo "  rm -rf data"
