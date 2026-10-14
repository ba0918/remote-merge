#!/usr/bin/env bash
set -euo pipefail

source_dir=$(cd "$(dirname "$0")" && pwd)
repo=$(cd "$source_dir/../.." && pwd)
scratch=$(mktemp -d "${TMPDIR:-/tmp}/remote-merge-legacy.XXXXXXXX")
name="remote-merge-legacy-$(basename "$scratch" | tr '[:upper:]' '[:lower:]')"
cleanup() {
    docker rm -f "$name" >/dev/null 2>&1 || true
    docker image rm "$name" >/dev/null 2>&1 || true
    rm -rf -- "$scratch"
}
trap cleanup EXIT

mkdir -p "$scratch/build/.ssh" "$scratch/local" "$scratch/home" "$scratch/data"
cp "$source_dir/Dockerfile" "$source_dir/sshd_config" "$scratch/build/"
cp -r "$source_dir/rpms" "$scratch/build/rpms"
ssh-keygen -q -t rsa -b 2048 -N '' -f "$scratch/build/.ssh/id_rsa"
docker build -t "$name" "$scratch/build"
docker run --rm -d --name "$name" -p 127.0.0.1::22 "$name" >/dev/null
port=$(docker port "$name" 22/tcp | head -1 | cut -d: -f2)
test -n "$port"

ssh_args=(-o BatchMode=yes -o ConnectTimeout=5 -o StrictHostKeyChecking=accept-new
    -o UserKnownHostsFile="$scratch/known_hosts" -o IdentitiesOnly=yes
    -o KexAlgorithms=+diffie-hellman-group14-sha1
    -o HostKeyAlgorithms=+ssh-rsa,ssh-dss
    -o PubkeyAcceptedAlgorithms=+ssh-rsa
    -i "$scratch/build/.ssh/id_rsa" -p "$port")
ready=false
for _ in $(seq 1 30); do
    if ssh "${ssh_args[@]}" testuser@127.0.0.1 true >/dev/null 2>&1; then
        ready=true
        break
    fi
    sleep 1
done
if [[ $ready != true ]]; then
    echo 'Legacy OpenSSH did not become ready' >&2
    exit 1
fi

docker cp "$source_dir/generate_testdata.sh" "$name:/tmp/generate_testdata.sh" >/dev/null
docker exec "$name" bash /tmp/generate_testdata.sh
REMOTE_MERGE_LEGACY_LOCAL_DIR="$scratch/local" REMOTE_MERGE_LEGACY_CONTAINER="$name" \
    bash "$source_dir/generate_local_data.sh"

cat > "$scratch/config.toml" <<EOF
[local]
root_dir = "$scratch/local"
[servers.centos5]
host = "127.0.0.1"
port = $port
user = "testuser"
auth = "key"
key = "$scratch/build/.ssh/id_rsa"
root_dir = "/srv/testdata"
[servers.centos5.ssh_options]
kex_algorithms = ["diffie-hellman-group14-sha1", "diffie-hellman-group1-sha1"]
[filter]
exclude = [".git", "node_modules"]
[ssh]
timeout_sec = 300
EOF

export REMOTE_MERGE_LEGACY_CONFIG="$scratch/config.toml"
export CARGO_HOME="${CARGO_HOME:-$HOME/.cargo}" RUSTUP_HOME="${RUSTUP_HOME:-$HOME/.rustup}"
export HOME="$scratch/home" XDG_CONFIG_HOME="$scratch/home/config" XDG_DATA_HOME="$scratch/data"
echo 'Legacy SSH trial ready. Exit the shell to remove its keys, data, image and container.'
echo 'Use: cargo run -- --config "$REMOTE_MERGE_LEGACY_CONFIG" status --right centos5'
cd "$repo"
if (( $# )); then
    "$@"
else
    bash --noprofile --norc -i
fi
