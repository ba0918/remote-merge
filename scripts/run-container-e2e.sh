#!/usr/bin/env bash
set -euo pipefail

repo=$(cd "$(dirname "$0")/.." && pwd)
image=${REMOTE_MERGE_E2E_IMAGE:-'linuxserver/openssh-server@sha256:f6ed4e429022c33a7590059b262b59d689c7ec48a0027179fad8ae3a1ad6222d'}

command -v docker >/dev/null
command -v ssh-keygen >/dev/null
command -v ssh >/dev/null
docker info >/dev/null
docker pull "$image" >/dev/null

cargo build --manifest-path "$repo/Cargo.toml" --bin remote-merge
cargo build --manifest-path "$repo/Cargo.toml" --release --target x86_64-unknown-linux-musl --bin remote-merge

scratch=$(mktemp -d "${TMPDIR:-/tmp}/remote-merge-openssh.XXXXXXXX")
permitted="remote-merge-e2e-$(basename "$scratch")"
rejected="${permitted}-deny"
cleanup() {
    docker exec -u root "$permitted" chown -R "$(id -u):$(id -g)" /config >/dev/null 2>&1 || true
    docker exec -u root "$rejected" chown -R "$(id -u):$(id -g)" /config >/dev/null 2>&1 || true
    docker rm -f "$permitted" "$rejected" >/dev/null 2>&1 || true
    rm -rf -- "$scratch"
}
trap cleanup EXIT

mkdir -p "$scratch/permit-config" "$scratch/deny-config" "$scratch/home" "$scratch/data"
ssh-keygen -q -t ed25519 -N '' -f "$scratch/id_ed25519"

start_server() {
    local name=$1 config=$2 sudo_access=$3
    docker run --rm -d --name "$name" -p 127.0.0.1::2222 \
        -e USER_NAME=testuser -e SUDO_ACCESS="$sudo_access" -e PASSWORD_ACCESS=false \
        -e PUBLIC_KEY="$(cat "$scratch/id_ed25519.pub")" \
        -v "$config:/config" "$image" >/dev/null
    if [[ ${REMOTE_MERGE_E2E_ABORT_AT:-} == sshd ]]; then
        echo 'Injected sshd readiness failure' >&2
        return 1
    fi
    local port
    port=$(docker port "$name" 2222/tcp | head -1 | cut -d: -f2)
    test -n "$port"
    local ready=false
    for _ in $(seq 1 30); do
        if ssh -o BatchMode=yes -o ConnectTimeout=2 -o StrictHostKeyChecking=yes \
            -o UserKnownHostsFile="$scratch/known_hosts" -o IdentitiesOnly=yes \
            -i "$scratch/id_ed25519" -p "$port" testuser@127.0.0.1 'true' >/dev/null 2>&1; then
            ready=true
            break
        fi
        ssh-keyscan -p "$port" -H 127.0.0.1 >> "$scratch/known_hosts" 2>/dev/null || true
        sleep 1
    done
    if [[ "$ready" != true ]]; then
        echo "OpenSSH did not become ready: $name" >&2
        return 1
    fi
    docker exec -u root "$name" apk add --no-cache openssl >/dev/null
    docker exec -u root "$name" sh -c 'mkdir -p /data /srv/testdata/remote-merge-e2e /home/testuser/remote-merge-e2e-shared && chown -R testuser:testuser /data /srv/testdata /home/testuser/remote-merge-e2e-shared'
    echo "$port"
}

permit_port=$(start_server "$permitted" "$scratch/permit-config" true)
deny_port=$(start_server "$rejected" "$scratch/deny-config" false)
docker exec -u root "$permitted" mkdir -p /tmp/agent/remote-merge-testuser
docker cp "$repo/target/x86_64-unknown-linux-musl/release/remote-merge" "$permitted:/tmp/agent/remote-merge-testuser/remote-merge" >/dev/null
docker exec -u root "$permitted" chmod 755 /tmp/agent/remote-merge-testuser/remote-merge

cat > "$scratch/server.toml" <<EOF
[servers.develop]
host = "127.0.0.1"
port = $permit_port
user = "testuser"
auth = "key"
key = "$scratch/id_ed25519"
root_dir = "/srv/testdata/remote-merge-e2e"
[local]
root_dir = "$scratch/home"
[ssh]
timeout_sec = 10
strict_host_key_checking = "no"
[backup]
enabled = true
[agent]
enabled = false
EOF

export REMOTE_MERGE_BINARY="$repo/target/debug/remote-merge"
export REMOTE_MERGE_AGENT_BINARY="$repo/target/x86_64-unknown-linux-musl/release/remote-merge"
export REMOTE_MERGE_E2E_SERVER_TOML="$scratch/server.toml"
export REMOTE_MERGE_E2E_CONTAINER="$permitted"
export REMOTE_MERGE_E2E_PORT="$permit_port"
export REMOTE_MERGE_E2E_REJECT_CONTAINER="$rejected"
export REMOTE_MERGE_E2E_REJECT_PORT="$deny_port"
export REMOTE_MERGE_KEY="$scratch/id_ed25519"
export HOME="$scratch/home" XDG_CONFIG_HOME="$scratch/home/config" XDG_DATA_HOME="$scratch/data"

if [[ ${REMOTE_MERGE_E2E_ABORT_AT:-} == test ]]; then
    echo 'Injected test failure' >&2
    exit 1
fi

cargo test --manifest-path "$repo/tests/container-e2e/Cargo.toml" --all-targets -- --test-threads=1
