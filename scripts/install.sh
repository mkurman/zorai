#!/bin/sh
# install.sh -- Install zorai native binaries without npm
# Usage: curl -fsSL https://raw.githubusercontent.com/mkurman/zorai/main/scripts/install.sh | sh
#        ZORAI_VERSION=0.4.2 ./scripts/install.sh
#        ZORAI_INSTALL_DIR=/opt/zorai/bin ./scripts/install.sh
#        ./scripts/install.sh --dry-run
set -eu

INSTALL_DIR="${ZORAI_INSTALL_DIR:-$HOME/.local/bin}"
SKILLS_DIR="${ZORAI_SKILLS_DIR:-$HOME/.zorai/skills}"
GUIDELINES_DIR="${ZORAI_GUIDELINES_DIR:-$HOME/.zorai/guidelines}"
BUNDLED_PLUGINS_DIR="${ZORAI_BUNDLED_PLUGINS_DIR:-$HOME/.zorai/bundled-plugins}"
GITHUB_OWNER="mkurman"
GITHUB_REPO="zorai"
GITHUB_API_URL="https://api.github.com/repos/${GITHUB_OWNER}/${GITHUB_REPO}"
DOWNLOAD_BASE_URL="https://github.com/${GITHUB_OWNER}/${GITHUB_REPO}/releases/download"
DIRECT_INSTALL_MARKER="${INSTALL_DIR}/.zorai-install-source"
BINARIES="zorai zorai-daemon zorai-tui zorai-gateway zorai-mcp zorai-desktop"
ASSETS=""
DRY_RUN=false

for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=true ;;
    *)
      echo "Error: Unsupported argument: $arg" >&2
      exit 1
      ;;
  esac
done

die() {
  echo "Error: $*" >&2
  exit 1
}

normalize_version() {
  printf '%s' "$1" | sed 's/^v//'
}

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    die "Required command not found: $1"
  fi
}

download_file() {
  curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    -H "User-Agent: zorai-installer" \
    "$1" \
    -o "$2"
}

detect_platform() {
  os_name="$(uname -s)"
  arch_name="$(uname -m)"

  case "$os_name" in
    Linux) platform="linux" ;;
    Darwin) platform="darwin" ;;
    *) die "Unsupported OS: $os_name" ;;
  esac

  case "$platform:$arch_name" in
    linux:x86_64|linux:amd64)
      archive_platform="linux-x86_64"
      checksum_platform="linux-x86_64"
      ;;
    linux:aarch64|linux:arm64)
      archive_platform="linux-aarch64"
      checksum_platform="linux-aarch64"
      ;;
    darwin:x86_64|darwin:amd64)
      archive_platform="darwin-x86_64"
      checksum_platform="darwin-x86_64"
      ASSETS="zorai-desktop.app.zip"
      ;;
    darwin:arm64|darwin:aarch64)
      archive_platform="darwin-arm64"
      checksum_platform="darwin-arm64"
      ASSETS="zorai-desktop.app.zip"
      ;;
    *) die "Unsupported architecture for $platform: $arch_name" ;;
  esac

  archive_name="zorai-${archive_platform}.zip"
  checksum_name="SHA256SUMS-${checksum_platform}.txt"

  echo "Detected platform: ${archive_platform}"
}

resolve_version() {
  if [ -n "${ZORAI_VERSION:-}" ]; then
    VERSION="$(normalize_version "$ZORAI_VERSION")"
    echo "Using specified version: ${VERSION}"
    return
  fi

  metadata="$(curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    -H "User-Agent: zorai-installer" \
    "${GITHUB_API_URL}/releases/latest")" || true

  VERSION="$(printf '%s' "$metadata" | tr -d '\n' | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"v\{0,1\}\([^"]*\)".*/\1/p' | head -n 1)"

  if [ -z "${VERSION:-}" ]; then
    die "Could not determine latest version from GitHub. Set ZORAI_VERSION=x.y.z"
  fi

  echo "Latest version: ${VERSION}"
}

wait_for_previous_zorai() {
  if [ -z "${ZORAI_UPGRADE_WAIT_PID:-}" ]; then
    return
  fi

  echo "Waiting for zorai process ${ZORAI_UPGRADE_WAIT_PID} to exit..."
  while kill -0 "${ZORAI_UPGRADE_WAIT_PID}" 2>/dev/null; do
    sleep 1
  done
}

migrate_legacy_tamux_root() {
  legacy_root="${HOME}/.tamux"
  target_root="${HOME}/.zorai"

  if [ -d "$legacy_root" ] && [ ! -e "$target_root" ]; then
    mv "$legacy_root" "$target_root"
    echo "Migrated legacy runtime data: ${legacy_root} -> ${target_root}"
  fi
}

sha256_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
    return
  fi

  if command -v shasum >/dev/null 2>&1; then
    shasum -a 256 "$1" | awk '{print $1}'
    return
  fi

  die "Need sha256sum or shasum to verify downloads"
}

lookup_checksum() {
  awk -v name="$1" '
    $NF == name || $NF == "*" name { print $1; exit }
  ' "$CHECKSUM_PATH"
}

verify_archive() {
  expected_hash="$(lookup_checksum "$archive_name")"

  if [ -z "$expected_hash" ]; then
    return 1
  fi

  actual_hash="$(sha256_file "$ARCHIVE_PATH")"
  if [ "$actual_hash" != "$expected_hash" ]; then
    die "SHA256 mismatch for ${archive_name}"
  fi

  return 0
}

extract_archive() {
  mkdir -p "$EXTRACT_DIR"

  if command -v unzip >/dev/null 2>&1; then
    unzip -oq "$ARCHIVE_PATH" -d "$EXTRACT_DIR"
    return
  fi

  if command -v bsdtar >/dev/null 2>&1; then
    bsdtar -xf "$ARCHIVE_PATH" -C "$EXTRACT_DIR"
    return
  fi

  die "Need unzip or bsdtar to extract ${archive_name}"
}

verify_binary() {
  binary_name="$1"
  expected_hash="$(lookup_checksum "$binary_name")"

  if [ -z "$expected_hash" ]; then
    die "Checksum not found for ${binary_name} in ${checksum_name}"
  fi

  actual_hash="$(sha256_file "$EXTRACT_DIR/$binary_name")"
  if [ "$actual_hash" != "$expected_hash" ]; then
    die "SHA256 mismatch for ${binary_name}"
  fi
}

verify_asset() {
  asset_name="$1"
  expected_hash="$(lookup_checksum "$asset_name")"

  if [ -z "$expected_hash" ]; then
    die "Checksum not found for ${asset_name} in ${checksum_name}"
  fi

  actual_hash="$(sha256_file "$EXTRACT_DIR/$asset_name")"
  if [ "$actual_hash" != "$expected_hash" ]; then
    die "SHA256 mismatch for ${asset_name}"
  fi
}

install_binaries() {
  verify_extracted_binaries="${1:-true}"
  mkdir -p "$INSTALL_DIR"

  for binary_name in $BINARIES; do
    if [ ! -f "$EXTRACT_DIR/$binary_name" ]; then
      die "Release bundle is missing required binary ${binary_name}"
    fi

    if [ "$verify_extracted_binaries" = true ]; then
      verify_binary "$binary_name"
    fi
    cp "$EXTRACT_DIR/$binary_name" "$INSTALL_DIR/.$binary_name.new"
    chmod 755 "$INSTALL_DIR/.$binary_name.new"
    mv -f "$INSTALL_DIR/.$binary_name.new" "$INSTALL_DIR/$binary_name"
  done

  {
    echo "source=direct"
    echo "install_dir=$INSTALL_DIR"
  } > "$DIRECT_INSTALL_MARKER"

  echo "Installed: ${BINARIES} -> ${INSTALL_DIR}"
}

install_assets() {
  verify_extracted_assets="${1:-true}"
  mkdir -p "$INSTALL_DIR"

  for asset_name in $ASSETS; do
    if [ ! -f "$EXTRACT_DIR/$asset_name" ]; then
      die "Release bundle is missing required asset ${asset_name}"
    fi

    if [ "$verify_extracted_assets" = true ]; then
      verify_asset "$asset_name"
    fi
    cp "$EXTRACT_DIR/$asset_name" "$INSTALL_DIR/$asset_name"
  done

  if [ -n "$ASSETS" ]; then
    echo "Installed assets: ${ASSETS} -> ${INSTALL_DIR}"
  fi
}

install_macos_desktop_app() {
  if [ "${platform:-}" != "darwin" ]; then
    return
  fi

  app_zip="$INSTALL_DIR/zorai-desktop.app.zip"
  app_dir="$INSTALL_DIR/zorai-desktop.app"
  if [ ! -f "$app_zip" ]; then
    die "Installed macOS desktop app archive is missing: ${app_zip}"
  fi

  rm -rf "$app_dir"
  if command -v ditto >/dev/null 2>&1; then
    ditto -x -k "$app_zip" "$INSTALL_DIR"
  elif command -v unzip >/dev/null 2>&1; then
    unzip -oq "$app_zip" -d "$INSTALL_DIR"
  else
    die "Need ditto or unzip to install ${app_zip}"
  fi

  if [ ! -x "$app_dir/Contents/MacOS/zorai" ]; then
    die "Installed macOS desktop app is missing executable: ${app_dir}/Contents/MacOS/zorai"
  fi

  echo "Installed macOS desktop app -> ${app_dir}"
}

install_cli_alias() {
  alias_path="${INSTALL_DIR}/zoi"
  rm -f "$alias_path"
  (
    cd "$INSTALL_DIR"
    ln -sf "zorai" "$alias_path"
  )
  echo "Installed CLI alias: zoi -> zorai"
}

install_skills() {
  if [ ! -d "$EXTRACT_DIR/skills" ]; then
    die "Release bundle is missing bundled skills"
  fi

  mkdir -p "$SKILLS_DIR"
  cp -R "$EXTRACT_DIR/skills/." "$SKILLS_DIR/"
  echo "Installed bundled skills -> ${SKILLS_DIR}"
}

install_guidelines() {
  guidelines_source="$EXTRACT_DIR/guidelines"
  if [ ! -d "$guidelines_source" ]; then
    die "Release bundle is missing bundled guidelines"
  fi

  mkdir -p "$GUIDELINES_DIR"
  (
    cd "$guidelines_source"
    find . -type f | while IFS= read -r relative_path; do
      relative_path="${relative_path#./}"
      target_path="$GUIDELINES_DIR/$relative_path"
      if [ -e "$target_path" ]; then
        continue
      fi
      mkdir -p "$(dirname "$target_path")"
      cp "$guidelines_source/$relative_path" "$target_path"
    done
  )
  echo "Installed missing bundled guidelines -> ${GUIDELINES_DIR}"
}

install_bundled_plugins() {
  if [ ! -d "$EXTRACT_DIR/plugins" ]; then
    echo "Release bundle has no bundled plugins; skipping"
    return
  fi

  rm -rf "$BUNDLED_PLUGINS_DIR"
  mkdir -p "$BUNDLED_PLUGINS_DIR"
  cp -R "$EXTRACT_DIR/plugins/." "$BUNDLED_PLUGINS_DIR/"
  echo "Staged bundled plugins -> ${BUNDLED_PLUGINS_DIR} (zorai installs new ones disabled on its next start)"
}

install_custom_auth_template() {
  root_dir="${HOME}/.zorai"
  custom_auth_path="${root_dir}/custom-auth.yaml"
  mkdir -p "$root_dir"

  if [ -e "$custom_auth_path" ]; then
    return
  fi

  cat > "$custom_auth_path" <<'YAML'
# Add named custom providers here. The daemon reloads this file before
# provider/model setup in the TUI and desktop app.
# Prefer api_key_env for secrets, for example:
# providers:
#   - id: local-openai
#     name: Local OpenAI-Compatible
#     default_base_url: http://127.0.0.1:11434/v1
#     default_model: llama3.3
#     api_key_env: LOCAL_OPENAI_API_KEY
#     models:
#       - id: llama3.3
#         name: Llama 3.3
#         context_window: 128000
providers: []
YAML
  echo "Created custom provider template -> ${custom_auth_path}"
}

start_daemon_after_upgrade() {
  if [ "${ZORAI_START_DAEMON_AFTER_INSTALL:-}" != "1" ]; then
    return
  fi

  daemon_path="${INSTALL_DIR}/zorai-daemon"
  if [ ! -x "$daemon_path" ]; then
    die "Installed daemon binary is missing or not executable: ${daemon_path}"
  fi

  echo "Starting zorai-daemon..."
  "$daemon_path" >/dev/null 2>&1 &
}

print_path_hint() {
  case ":${PATH:-}:" in
    *":$INSTALL_DIR:"*)
      echo "zorai: run 'zorai --help' to confirm the install."
      ;;
    *)
      echo "zorai: add '${INSTALL_DIR}' to PATH if 'zorai' is not found."
      echo "zorai: example: export PATH=\"${INSTALL_DIR}:\$PATH\""
      echo "zorai: open a new shell after updating PATH, then run 'zorai --help'."
      ;;
  esac
}

cleanup() {
  if [ -n "${TMP_DIR:-}" ] && [ -d "$TMP_DIR" ]; then
    rm -rf "$TMP_DIR"
  fi
}

detect_platform
resolve_version

ARCHIVE_URL="${DOWNLOAD_BASE_URL}/v${VERSION}/${archive_name}"
CHECKSUM_URL="${DOWNLOAD_BASE_URL}/v${VERSION}/${checksum_name}"

if [ "$DRY_RUN" = true ]; then
  echo ""
  echo "Platform: ${archive_platform}"
  echo "Version: ${VERSION}"
  echo "Download URL: ${ARCHIVE_URL}"
  echo "Checksum URL: ${CHECKSUM_URL}"
  echo "Install directory: ${INSTALL_DIR}"
  echo "Skills directory: ${SKILLS_DIR}"
  echo "Guidelines directory: ${GUIDELINES_DIR}"
  echo "Binaries: ${BINARIES}"
  echo "Assets: ${ASSETS:-none}"
  echo "Dry run complete -- no files downloaded or modified."
  exit 0
fi

require_command curl
TMP_DIR="$(mktemp -d 2>/dev/null || mktemp -d -t zorai-install)"
ARCHIVE_PATH="${TMP_DIR}/${archive_name}"
CHECKSUM_PATH="${TMP_DIR}/${checksum_name}"
EXTRACT_DIR="${TMP_DIR}/extract"
trap cleanup EXIT INT TERM HUP

wait_for_previous_zorai
migrate_legacy_tamux_root

echo "Downloading zorai v${VERSION} for ${archive_platform}..."
download_file "$CHECKSUM_URL" "$CHECKSUM_PATH"
download_file "$ARCHIVE_URL" "$ARCHIVE_PATH"

verify_extracted_binaries=true
if verify_archive; then
  verify_extracted_binaries=false
fi

echo "Extracting binaries, skills, and guidelines..."
extract_archive

echo "Verifying extracted binaries..."
install_binaries "$verify_extracted_binaries"
install_assets "$verify_extracted_binaries"
install_macos_desktop_app
install_cli_alias
install_skills
install_guidelines
install_bundled_plugins
install_custom_auth_template
start_daemon_after_upgrade

echo ""
echo "zorai installed successfully."
print_path_hint
