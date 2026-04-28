#!/usr/bin/env bash
set -euo pipefail

REPO="puppycorp/puppylog"
BIN_NAME="plog"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
VERSION="${VERSION:-latest}"

need_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "error: required command not found: $1" >&2
        exit 1
    fi
}

need_cmd curl
need_cmd tar
need_cmd mktemp

os_name() {
    case "$(uname -s)" in
        Linux) echo "unknown-linux-gnu" ;;
        Darwin) echo "apple-darwin" ;;
        *)
            echo "error: unsupported OS $(uname -s)" >&2
            exit 1
            ;;
    esac
}

arch_name() {
    case "$(uname -m)" in
        x86_64|amd64) echo "x86_64" ;;
        arm64|aarch64) echo "aarch64" ;;
        *)
            echo "error: unsupported architecture $(uname -m)" >&2
            exit 1
            ;;
    esac
}

asset_target() {
    local os arch
    os="$(os_name)"
    arch="$(arch_name)"
    case "$arch-$os" in
        x86_64-unknown-linux-gnu) echo "x86_64-unknown-linux-gnu" ;;
        aarch64-apple-darwin) echo "aarch64-apple-darwin" ;;
        *)
            echo "error: no published plog release asset for $arch-$os" >&2
            exit 1
            ;;
    esac
}

resolve_version() {
    if [ "$VERSION" != "latest" ]; then
        echo "$VERSION"
        return
    fi

    curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
        | sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' \
        | head -n 1
}

verify_sha256() {
    local file="$1"
    local checksum_file="$2"
    local file_name

    file_name="$(basename "$file")"
    sed -i.bak "s#dist/${file_name}#${file_name}#g" "$checksum_file"
    rm -f "${checksum_file}.bak"

    if command -v sha256sum >/dev/null 2>&1; then
        sha256sum -c "$checksum_file"
    elif command -v shasum >/dev/null 2>&1; then
        shasum -a 256 -c "$checksum_file"
    else
        echo "warning: sha256sum/shasum not found, skipping checksum verification" >&2
    fi
}

main() {
    local version target archive_name asset_url checksum_url tmpdir archive_path checksum_path extracted_bin

    target="$(asset_target)"
    version="$(resolve_version)"

    if [ -z "$version" ]; then
        echo "error: failed to resolve release version" >&2
        exit 1
    fi

    archive_name="${BIN_NAME}-${version}-${target}.tar.gz"
    asset_url="https://github.com/${REPO}/releases/download/${version}/${archive_name}"
    checksum_url="${asset_url}.sha256"

    tmpdir="$(mktemp -d)"
    trap "rm -rf '$tmpdir'" EXIT

    archive_path="$tmpdir/$archive_name"
    checksum_path="$tmpdir/${archive_name}.sha256"

    echo "Installing ${BIN_NAME} ${version} for ${target}..."
    curl -fL "$asset_url" -o "$archive_path"
    curl -fL "$checksum_url" -o "$checksum_path"

    (
        cd "$tmpdir"
        verify_sha256 "$archive_path" "$checksum_path"
    )

    tar -xzf "$archive_path" -C "$tmpdir"
    extracted_bin="$tmpdir/$BIN_NAME"

    if [ ! -f "$extracted_bin" ]; then
        echo "error: archive did not contain $BIN_NAME" >&2
        exit 1
    fi

    mkdir -p "$INSTALL_DIR"
    install -m 0755 "$extracted_bin" "$INSTALL_DIR/$BIN_NAME"

    echo "Installed to $INSTALL_DIR/$BIN_NAME"
    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            echo ""
            echo "Add this to your shell profile if needed:"
            echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
            ;;
    esac
}

main "$@"
