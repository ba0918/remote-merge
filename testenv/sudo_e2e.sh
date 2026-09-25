#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
image=linuxserver/openssh-server:latest
docker image inspect "$image" >/dev/null
cargo build --bin remote-merge --manifest-path "$repo/Cargo.toml"
cargo build --release --target x86_64-unknown-linux-musl --bin remote-merge --manifest-path "$repo/Cargo.toml"

scratch=$(mktemp -d "${TMPDIR:-/tmp}/remote-merge-sudo.XXXXXXXX")
container="remote-merge-sudo-$(basename "$scratch")"
cleanup() {
    docker exec -u root "$container" chown -R "$(id -u):$(id -g)" /config >/dev/null 2>&1 || true
    docker stop "$container" >/dev/null 2>&1 || true
    rm -rf -- "$scratch"
}
trap cleanup EXIT

mkdir -p "$scratch/config" "$scratch/home" "$scratch/local" "$scratch/data"
ssh-keygen -q -t ed25519 -N '' -f "$scratch/id_ed25519"
printf 'incoming privileged\n' > "$scratch/local/example.txt"

docker run --rm -d --name "$container" -p 127.0.0.1::2222 \
    -e USER_NAME=testuser -e SUDO_ACCESS=true -e PASSWORD_ACCESS=false \
    -e PUBLIC_KEY="$(cat "$scratch/id_ed25519.pub")" \
    -v "$scratch/config:/config" "$image" >/dev/null
port=$(docker port "$container" 2222/tcp | head -1 | cut -d: -f2)

connected=false
for _ in $(seq 1 30); do
    if ssh -o BatchMode=yes -o ConnectTimeout=2 -o StrictHostKeyChecking=accept-new \
        -o UserKnownHostsFile="$scratch/known_hosts" -o IdentitiesOnly=yes \
        -i "$scratch/id_ed25519" -p "$port" testuser@127.0.0.1 \
        'sudo -n true' >/dev/null 2>&1; then
        connected=true
        break
    fi
    sleep 1
done
if [[ $connected != true ]]; then
    echo 'SSH with noninteractive sudo did not become ready' >&2
    exit 1
fi

docker exec -u root "$container" sh -c \
    'mkdir -p /data /tmp/agent/remote-merge-testuser && printf "old root-only\n" > /data/example.txt && chown root:root /data/example.txt && chmod 600 /data/example.txt && chmod 755 /data'
docker cp "$repo/target/x86_64-unknown-linux-musl/release/remote-merge" \
    "$container:/tmp/agent/remote-merge-testuser/remote-merge" >/dev/null
docker exec -u root "$container" chmod 755 /tmp/agent/remote-merge-testuser/remote-merge

cat > "$scratch/config.toml" <<EOF
[local]
root_dir = "$scratch/local"
[servers.fixture]
host = "127.0.0.1"
port = $port
user = "testuser"
auth = "key"
key = "$scratch/id_ed25519"
root_dir = "/data"
sudo = true
[agent]
enabled = true
deploy_dir = "/tmp/agent"
[backup]
enabled = true
[ssh]
timeout_sec = 10
strict_host_key_checking = "no"
EOF

export HOME="$scratch/home" XDG_CONFIG_HOME="$scratch/home/config" XDG_DATA_HOME="$scratch/data"
binary="$repo/target/debug/remote-merge"
"$binary" diff example.txt --config "$scratch/config.toml" \
    --left local --right fixture --format json > "$scratch/diff.json" || diff_status=$?
if [[ ${diff_status:-0} -ne 1 ]]; then
    echo 'Expected a difference before the privileged merge' >&2
    exit 1
fi
"$binary" merge example.txt --config "$scratch/config.toml" \
    --left local --right fixture --format json > "$scratch/merge.json"

python3 - "$scratch" <<'PY'
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
diff = json.loads((root / "diff.json").read_text())
assert "old root-only" in str(diff) and "incoming privileged" in str(diff), diff
merge = json.loads((root / "merge.json").read_text())
assert len(merge["merged"]) == 1 and not merge["failed"], merge
assert merge["merged"][0]["backup"], merge
backups = root / "data/remote-merge/backups"
assert any(path.read_bytes() == b"old root-only\n" for path in backups.rglob("content"))
PY

result=$(docker exec -u root "$container" sh -c \
    'test "$(stat -c %U /data/example.txt)" = root && test "$(stat -c %a /data/example.txt)" = 600 && cat /data/example.txt')
[[ $result == 'incoming privileged' ]]
echo 'sudo e2e: diff, privileged Agent merge, backup and root-only destination verified'
