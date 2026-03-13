#!/usr/bin/env sh
# remote-merge installer
# Usage: curl -fsSL https://raw.githubusercontent.com/ba0918/remote-merge/main/scripts/install.sh | sh
set -eu

REPO="ba0918/remote-merge"
BINARY="remote-merge"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# --- Helper functions ---

info() {
    printf '\033[1;34m==>\033[0m %s\n' "$1"
}

error() {
    printf '\033[1;31merror:\033[0m %s\n' "$1" >&2
    exit 1
}

need_cmd() {
    if ! command -v "$1" > /dev/null 2>&1; then
        error "required command not found: $1"
    fi
}

# --- Detect platform ---

detect_target() {
    local os arch target

    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)
            # Always use musl for maximum portability (works on glibc too)
            case "$arch" in
                x86_64)  target="x86_64-unknown-linux-musl" ;;
                *)       error "unsupported architecture: $arch (Linux)" ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64)  target="x86_64-apple-darwin" ;;
                arm64)   target="aarch64-apple-darwin" ;;
                *)       error "unsupported architecture: $arch (macOS)" ;;
            esac
            ;;
        *)
            error "unsupported OS: $os (use WSL on Windows)"
            ;;
    esac

    echo "$target"
}

# --- Resolve version ---

resolve_version() {
    local version="${VERSION:-}"

    if [ -n "$version" ]; then
        echo "$version"
        return
    fi

    # Fetch latest release tag from GitHub API
    local latest
    latest="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name"' \
        | sed -E 's/.*"tag_name":\s*"([^"]+)".*/\1/')" \
        || error "failed to fetch latest release from GitHub"

    if [ -z "$latest" ]; then
        error "could not determine latest version"
    fi

    echo "$latest"
}

# --- Download & verify ---

download_and_install() {
    local version="$1"
    local target="$2"
    local archive="${BINARY}-${target}.tar.gz"
    local base_url="https://github.com/${REPO}/releases/download/${version}"
    local tmpdir

    tmpdir="$(mktemp -d)"
    trap 'rm -rf "$tmpdir"' EXIT

    info "downloading ${BINARY} ${version} for ${target}..."
    curl -fSL "${base_url}/${archive}" -o "${tmpdir}/${archive}" \
        || error "failed to download ${archive}"

    # Verify SHA256 checksum
    info "verifying checksum..."
    curl -fsSL "${base_url}/SHA256SUMS.txt" -o "${tmpdir}/SHA256SUMS.txt" \
        || error "failed to download SHA256SUMS.txt"

    local expected actual
    expected="$(grep "${archive}" "${tmpdir}/SHA256SUMS.txt" | awk '{print $1}')"
    if [ -z "$expected" ]; then
        error "checksum not found for ${archive} in SHA256SUMS.txt"
    fi

    if command -v sha256sum > /dev/null 2>&1; then
        actual="$(sha256sum "${tmpdir}/${archive}" | awk '{print $1}')"
    elif command -v shasum > /dev/null 2>&1; then
        actual="$(shasum -a 256 "${tmpdir}/${archive}" | awk '{print $1}')"
    else
        error "neither sha256sum nor shasum found — cannot verify checksum"
    fi

    if [ "$expected" != "$actual" ]; then
        error "checksum mismatch!\n  expected: ${expected}\n  actual:   ${actual}"
    fi
    info "checksum OK"

    # Extract
    info "extracting..."
    tar xzf "${tmpdir}/${archive}" -C "${tmpdir}"

    # Install
    if [ -w "$INSTALL_DIR" ]; then
        mv "${tmpdir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
    else
        info "installing to ${INSTALL_DIR} (requires sudo)..."
        sudo mv "${tmpdir}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
    fi
    chmod +x "${INSTALL_DIR}/${BINARY}"

    info "installed ${BINARY} ${version} to ${INSTALL_DIR}/${BINARY}"
}

# --- Main ---

main() {
    need_cmd curl
    need_cmd tar
    need_cmd uname

    local target version
    target="$(detect_target)"
    version="$(resolve_version)"

    info "detected platform: ${target}"
    info "version: ${version}"

    download_and_install "$version" "$target"

    printf '\n\033[1;32mInstallation complete!\033[0m\n'
    printf 'Run \033[1m%s --help\033[0m to get started.\n' "$BINARY"
}

main "$@"
