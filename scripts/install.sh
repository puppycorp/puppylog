#!/usr/bin/env bash
set -euo pipefail

REPO="${REPO:-puppycorp/puppylog}"
BIN_NAME="puppylog"
SERVICE_NAME="${SERVICE_NAME:-puppylog}"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
CONFIG_DIR="${CONFIG_DIR:-${XDG_CONFIG_HOME:-$HOME/.config}/puppylog}"
DATA_DIR="${DATA_DIR:-${XDG_DATA_HOME:-$HOME/.local/share}/puppylog}"
VERSION="${VERSION:-latest}"
REPO_REF="${REPO_REF:-${DEV_BRANCH:-master}}"
START_SERVICE="${START_SERVICE:-1}"
ENABLE_SERVICE="${ENABLE_SERVICE:-1}"

need_cmd() {
	if ! command -v "$1" >/dev/null 2>&1; then
		echo "error: required command not found: $1" >&2
		exit 1
	fi
}

linux_target() {
	case "$(uname -s)" in
		Linux) ;;
		*)
			echo "error: puppylog service installer only supports Linux systemd user services" >&2
			exit 1
			;;
	esac

	case "$(uname -m)" in
		x86_64|amd64) echo "x86_64-unknown-linux-gnu" ;;
		*)
			echo "error: no published puppylog server release asset for $(uname -m)-linux" >&2
			exit 1
			;;
	esac
}

assert_linux() {
	case "$(uname -s)" in
		Linux) ;;
		*)
			echo "error: puppylog service installer only supports Linux systemd user services" >&2
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

quote_env() {
	local value="${1//\\/\\\\}"
	value="${value//\"/\\\"}"
	printf '"%s"' "$value"
}

download_binary() {
	local target version archive_name asset_url checksum_url tmpdir archive_path checksum_path extracted_bin

	need_cmd curl
	need_cmd tar
	need_cmd mktemp

	target="$(linux_target)"
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

	install -m 0755 "$extracted_bin" "$INSTALL_DIR/$BIN_NAME"
}

install_repo_binary() {
	local tmpdir source_dir built_bin

	assert_linux
	need_cmd cargo
	need_cmd git
	need_cmd mktemp

	tmpdir="$(mktemp -d)"
	trap "rm -rf '$tmpdir'" EXIT
	source_dir="$tmpdir/source"

	echo "Building ${BIN_NAME} from ${REPO} ref ${REPO_REF}..."
	git clone --depth 1 --branch "$REPO_REF" "https://github.com/${REPO}.git" "$source_dir"

	(
		cd "$source_dir"
		cargo build --release --bin "$BIN_NAME"
	)

	built_bin="$source_dir/target/release/$BIN_NAME"
	if [ ! -f "$built_bin" ]; then
		echo "error: build did not produce $built_bin" >&2
		exit 1
	fi

	install -m 0755 "$built_bin" "$INSTALL_DIR/$BIN_NAME"
}

install_binary() {
	mkdir -p "$INSTALL_DIR"

	if [ -n "${PUPPYLOG_BIN:-}" ]; then
		if [ ! -f "$PUPPYLOG_BIN" ]; then
			echo "error: PUPPYLOG_BIN does not exist: $PUPPYLOG_BIN" >&2
			exit 1
		fi

		echo "Installing ${BIN_NAME} from ${PUPPYLOG_BIN}..."
		install -m 0755 "$PUPPYLOG_BIN" "$INSTALL_DIR/$BIN_NAME"
	elif [ "$VERSION" = "repo" ] || [ "$VERSION" = "dev" ]; then
		install_repo_binary
	else
		download_binary
	fi
}

write_env_file() {
	local env_file="$CONFIG_DIR/${SERVICE_NAME}.env"
	local log_path="${LOG_PATH:-$DATA_DIR/logs}"
	local db_path="${DB_PATH:-$DATA_DIR/puppylog.db}"
	local settings_path="${SETTINGS_PATH:-$DATA_DIR/settings.json}"
	local upload_path="${UPLOAD_PATH:-$DATA_DIR/uploads}"

	mkdir -p "$CONFIG_DIR" "$DATA_DIR" "$log_path" "$upload_path" "$(dirname "$db_path")" "$(dirname "$settings_path")"

	cat > "$env_file" <<EOF
LOG_PATH=$(quote_env "$log_path")
DB_PATH=$(quote_env "$db_path")
SETTINGS_PATH=$(quote_env "$settings_path")
UPLOAD_PATH=$(quote_env "$upload_path")
MERGER_RUN=$(quote_env "${MERGER_RUN:-true}")
RUN_SEGMENT_COMPACTOR=$(quote_env "${RUN_SEGMENT_COMPACTOR:-true}")
EOF

	echo "$env_file"
}

write_service_file() {
	local env_file="$1"
	local user_systemd_dir="${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user"
	local service_file="$user_systemd_dir/${SERVICE_NAME}.service"

	mkdir -p "$user_systemd_dir"

	cat > "$service_file" <<EOF
[Unit]
Description=PuppyLog server
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
EnvironmentFile=$env_file
WorkingDirectory=$DATA_DIR
ExecStart=$INSTALL_DIR/$BIN_NAME
Restart=on-failure
RestartSec=5

[Install]
WantedBy=default.target
EOF

	echo "$service_file"
}

reload_user_systemd() {
	local service_unit="$1"

	if ! command -v systemctl >/dev/null 2>&1; then
		echo "warning: systemctl not found; service files were installed but not loaded" >&2
		return
	fi

	if ! systemctl --user daemon-reload; then
		echo "warning: systemctl --user daemon-reload failed; service files were installed" >&2
		return
	fi

	if [ "$ENABLE_SERVICE" = "1" ]; then
		systemctl --user enable "$service_unit"
	fi

	if [ "$START_SERVICE" = "1" ]; then
		systemctl --user restart "$service_unit"
	fi
}

main() {
	local env_file service_file service_unit

	install_binary
	env_file="$(write_env_file)"
	service_file="$(write_service_file "$env_file")"
	service_unit="$(basename "$service_file")"
	reload_user_systemd "$service_unit"

	echo "Installed $BIN_NAME to $INSTALL_DIR/$BIN_NAME"
	echo "Installed environment file to $env_file"
	echo "Installed user service to $service_file"
	echo "PuppyLog listens on http://127.0.0.1:3337 by default"
	echo ""
	echo "Edit service settings:"
	echo "  ${EDITOR:-vi} $env_file"
	echo "  systemctl --user restart $service_unit"
	echo ""
	echo "Check service status:"
	echo "  systemctl --user status $service_unit"
	echo ""
	echo "Keep the user service running after logout:"
	echo "  sudo loginctl enable-linger ${USER:-$(id -un)}"
}

main "$@"
