#!/usr/bin/env bash
# ============================================================================
# zorai release build (Linux / macOS / WSL)
#
# Usage:
#   ./scripts/build-release.sh                    Build without signing
#   ./scripts/build-release.sh --sign             Build and sign (GPG)
#   ./scripts/build-release.sh --skip-rust        Skip Rust compilation
#   ./scripts/build-release.sh --skip-frontend    Skip frontend build
#   ./scripts/build-release.sh --skip-electron    Skip Electron packaging
#   ./scripts/build-release.sh --target <triple>  Cross-compile for target
# ============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

export ZORAI_LOG=error
export ZORAI_TUI_LOG=error
export ZORAI_GATEWAY_LOG=error
export RUST_LOG=error

SIGN=0
SKIP_RUST=0
SKIP_FRONTEND=0
SKIP_ELECTRON=0
TARGET=""

while [[ $# -gt 0 ]]; do
    case "$1" in
        --sign)          SIGN=1; shift ;;
        --skip-rust)     SKIP_RUST=1; shift ;;
        --skip-frontend) SKIP_FRONTEND=1; shift ;;
        --skip-electron) SKIP_ELECTRON=1; shift ;;
        --target)        TARGET="$2"; shift 2 ;;
        *)               echo "Unknown option: $1"; exit 1 ;;
    esac
done

step=0
total=6

step_msg() {
    step=$((step + 1))
    echo ""
    echo "[$step/$total] $1"
}

ok_msg() {
    echo "  $1"
}

warn_msg() {
    echo "  WARNING: $1"
}

npm_ci_with_retries() {
    npm config set fetch-retries 5
    npm config set fetch-retry-mintimeout 20000
    npm config set fetch-retry-maxtimeout 120000

    local max_attempts=3
    local attempt
    for attempt in $(seq 1 "$max_attempts"); do
        if npm ci --prefer-offline --no-audit; then
            return 0
        fi
        if [[ "$attempt" -eq "$max_attempts" ]]; then
            return 1
        fi
        warn_msg "npm ci failed on attempt $attempt of $max_attempts; retrying after transient npm/network failure."
        sleep $((15 * attempt))
    done
}

APP_VERSION="$(sed -nE 's/^[[:space:]]*"version":[[:space:]]*"([^"]+)".*/\1/p' "$PROJECT_ROOT/frontend/package.json" | head -1)"
if [[ -z "$APP_VERSION" ]]; then
    APP_VERSION="0.0.0"
fi

ARCH="$(uname -m)"

checksum_file() {
    local file="$1"
    if command -v sha256sum &>/dev/null; then
        sha256sum "$file" | awk '{print $1}'
    elif command -v shasum &>/dev/null; then
        shasum -a 256 "$file" | awk '{print $1}'
    else
        python3 - "$file" <<'PY'
import hashlib
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
print(hashlib.sha256(path.read_bytes()).hexdigest())
PY
    fi
}

generate_release_notes_if_missing() {
    local notes_file="$1"
    shift
    local artifacts=("$@")

    [[ -f "$notes_file" ]] && return 0

    {
        echo "# zorai ${APP_VERSION} Release Notes"
        echo ""
        echo "Built on $(date -u +"%Y-%m-%d %H:%M UTC") for ${OS} (${ARCH})."
        echo ""
        echo "## Highlights"
        echo ""
        echo "- Rebrand alignment across runtime, packaging, and release artifacts."
        echo "- Updated release packaging flow with zipped bundles and SHA-256 manifests."
        echo "- Browser panel layout and resize fixes in the Electron frontend."
        echo ""
        echo "## Included Artifacts"
        echo ""
        for artifact in "${artifacts[@]}"; do
            echo "- ${artifact}"
        done
    } > "$notes_file"
}

write_checksums_file() {
    local output_file="$1"
    shift
    local artifacts=("$@")

    : > "$output_file"
    for artifact in "${artifacts[@]}"; do
        local hash
        hash="$(checksum_file "$OUT_DIR/$artifact")"
        printf "%s  %s\n" "$hash" "$artifact" >> "$output_file"
    done
}

create_bundle_zip() {
    local zip_path="$1"
    shift
    local files=("$@")

    python3 - "$zip_path" "$OUT_DIR" "$PROJECT_ROOT" "${files[@]}" <<'PY'
import os
import pathlib
import sys
import zipfile

zip_path = pathlib.Path(sys.argv[1])
base_dir = pathlib.Path(sys.argv[2])
project_root = pathlib.Path(sys.argv[3])
files = sys.argv[4:]

with zipfile.ZipFile(zip_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
    for relative in files:
        file_path = base_dir / relative
        archive.write(file_path, arcname=relative)

    skills_root = project_root / "skills"
    if skills_root.exists():
        for path in sorted(skills_root.rglob("*")):
            if path.is_file():
                archive.write(path, arcname=str(path.relative_to(project_root)).replace(os.sep, "/"))

    guidelines_root = project_root / "guidelines"
    if guidelines_root.exists():
        for path in sorted(guidelines_root.rglob("*")):
            if path.is_file():
                archive.write(path, arcname=str(path.relative_to(project_root)).replace(os.sep, "/"))

    plugins_root = project_root / "plugins"
    skipped = {"node_modules", "__pycache__", ".pytest_cache", "tests", ".python", ".venv"}
    if plugins_root.exists():
        for path in sorted(plugins_root.rglob("*")):
            relative = path.relative_to(project_root)
            if path.is_file() and not skipped.intersection(relative.parts):
                archive.write(path, arcname=str(relative).replace(os.sep, "/"))
PY
}

# Detect platform
OS="$(uname -s)"
OS_SLUG="$(printf '%s' "$OS" | tr '[:upper:]' '[:lower:]')"
EXE=""
PLATFORM_DIR="linux"
if [[ "$OS" == *MINGW* ]] || [[ "$OS" == *MSYS* ]] || [[ "$OS" == *CYGWIN* ]]; then
    EXE=".exe"
    PLATFORM_DIR="windows"
elif [[ "$OS" == "Darwin" ]]; then
    PLATFORM_DIR="macos"
fi

OUT_DIR="$PROJECT_ROOT/dist-release/$PLATFORM_DIR"

echo ""
echo "============================================================"
echo " zorai release build"
echo "============================================================"

# -----------------------------------------------------------
# Setup preflight
# -----------------------------------------------------------
echo ""
echo "[preflight] Checking required setup dependencies..."
"$SCRIPT_DIR/setup.sh" --check --profile source --format text
ok_msg "Setup preflight complete"

# -----------------------------------------------------------
# Step 1: Rust
# -----------------------------------------------------------
if [[ $SKIP_RUST -eq 1 ]]; then
    step_msg "Skipping Rust build (--skip-rust)"
else
    step_msg "Building Rust binaries (release)..."
    cd "$PROJECT_ROOT"
    CARGO_ARGS=(build --release)
    if [[ -n "$TARGET" ]]; then
        CARGO_ARGS+=(--target "$TARGET")
    fi
    cargo "${CARGO_ARGS[@]}"
    ok_msg "Rust build complete"
fi

# -----------------------------------------------------------
# Step 2: Frontend
# -----------------------------------------------------------
if [[ $SKIP_FRONTEND -eq 1 ]]; then
    step_msg "Skipping frontend build (--skip-frontend)"
else
    step_msg "Building frontend..."
    cd "$PROJECT_ROOT/frontend"
    npm_ci_with_retries
    npm run build
    ok_msg "Frontend build complete"
fi

# -----------------------------------------------------------
# Step 3: Collect artifacts
# -----------------------------------------------------------
step_msg "Collecting artifacts..."
mkdir -p "$OUT_DIR"
find "$OUT_DIR" -maxdepth 1 -type f \( -name "zorai*" -o -name "zorai*" -o -name "*.asc" -o -name "SHA256SUMS*.txt" -o -name "RELEASE_NOTES*.md" \) -delete 2>/dev/null || true

if [[ -n "$TARGET" ]]; then
    TARGET_DIR="$PROJECT_ROOT/target/$TARGET/release"
else
    TARGET_DIR="$PROJECT_ROOT/target/release"
fi

for bin in zorai-daemon zorai zoi zorai-tui zorai-mcp zorai-gateway; do
    if [[ -f "$TARGET_DIR/${bin}${EXE}" ]]; then
        cp "$TARGET_DIR/${bin}${EXE}" "$OUT_DIR/"
        ok_msg "Collected ${bin}${EXE}"
    fi
done

# Copy to frontend/dist for Electron bundling
if [[ -d "$PROJECT_ROOT/frontend/dist" ]]; then
    for bin in zorai-daemon zorai zoi zorai-tui zorai-mcp zorai-gateway; do
        if [[ -f "$OUT_DIR/${bin}${EXE}" ]]; then
            cp "$OUT_DIR/${bin}${EXE}" "$PROJECT_ROOT/frontend/dist/"
        fi
    done
fi

if [[ -f "$PROJECT_ROOT/docs/getting-started.md" ]]; then
    cp "$PROJECT_ROOT/docs/getting-started.md" "$OUT_DIR/GETTING_STARTED.md"
    if [[ -d "$PROJECT_ROOT/frontend/dist" ]]; then
        cp "$PROJECT_ROOT/docs/getting-started.md" "$PROJECT_ROOT/frontend/dist/GETTING_STARTED.md"
    fi
    ok_msg "Collected GETTING_STARTED.md"
fi

# -----------------------------------------------------------
# Step 4: Code signing
# -----------------------------------------------------------
if [[ $SIGN -eq 1 ]]; then
    step_msg "Signing binaries..."

    sign_file() {
        local file="$1"
        [[ -f "$file" ]] || return 0

        local name
        name="$(basename "$file")"

        if [[ -n "$EXE" ]] && command -v signtool &>/dev/null; then
            # Windows signing via signtool
            if [[ -n "${ZORAI_SIGN_CERT:-${ZORAI_SIGN_CERT:-}}" ]]; then
                signtool sign /f "${ZORAI_SIGN_CERT:-${ZORAI_SIGN_CERT:-}}" /p "${ZORAI_SIGN_PASSWORD:-${ZORAI_SIGN_PASSWORD:-}}" \
                    /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 "$file"
                ok_msg "Signed $name (signtool/PFX)"
            elif [[ -n "${ZORAI_SIGN_THUMBPRINT:-${ZORAI_SIGN_THUMBPRINT:-}}" ]]; then
                signtool sign /sha1 "${ZORAI_SIGN_THUMBPRINT:-${ZORAI_SIGN_THUMBPRINT:-}}" \
                    /fd SHA256 /tr http://timestamp.digicert.com /td SHA256 "$file"
                ok_msg "Signed $name (signtool/store)"
            else
                warn_msg "No Windows signing cert configured for $name"
            fi
        elif command -v gpg &>/dev/null; then
            # Linux/macOS signing via GPG
            gpg --detach-sign --armor "$file"
            ok_msg "Signed $name (GPG detached sig: ${name}.asc)"
        elif command -v codesign &>/dev/null && [[ "$OS" == "Darwin" ]]; then
            # macOS signing
            local identity="${ZORAI_SIGN_IDENTITY:-${ZORAI_SIGN_IDENTITY:-}}"
            if [[ -n "$identity" ]]; then
                codesign --sign "$identity" --timestamp --options runtime "$file"
                ok_msg "Signed $name (codesign)"
            else
                warn_msg "Set ZORAI_SIGN_IDENTITY for macOS codesign"
            fi
        else
            warn_msg "No signing tool available for $name"
        fi
    }

    for f in "$OUT_DIR"/*; do
        [[ -f "$f" ]] && sign_file "$f"
    done
    ok_msg "Signing complete"
else
    step_msg "Skipping code signing (pass --sign to enable)"
fi

# -----------------------------------------------------------
# Step 5: Electron packaging
# -----------------------------------------------------------
if [[ $SKIP_ELECTRON -eq 1 ]]; then
    step_msg "Skipping Electron build (--skip-electron)"
else
    step_msg "Building Electron app..."
    cd "$PROJECT_ROOT/frontend"

    if [[ $SIGN -eq 1 ]]; then
        # Pass signing certs to electron-builder
        export CSC_LINK="${ZORAI_SIGN_CERT:-${ZORAI_SIGN_CERT:-}}"
        export CSC_KEY_PASSWORD="${ZORAI_SIGN_PASSWORD:-${ZORAI_SIGN_PASSWORD:-}}"
    fi

    case "$OS" in
        Linux*)
            find "$PROJECT_ROOT/frontend/release" -maxdepth 1 -type f \( -name "zorai*" -o -name "zorai*" \) -delete 2>/dev/null || true
            ELECTRON_ARGS=(--linux AppImage deb)
            if [[ "$TARGET" == "aarch64-unknown-linux-gnu" ]]; then
                ELECTRON_ARGS+=(--arm64)
            fi
            npx electron-builder "${ELECTRON_ARGS[@]}" || warn_msg "Electron Linux build failed (non-fatal)"
            ;;
        Darwin*)
            find "$PROJECT_ROOT/frontend/release" -maxdepth 1 -type f \( -name "zorai*" -o -name "zorai*" \) -delete 2>/dev/null || true
            npx electron-builder --mac dmg || warn_msg "Electron macOS build failed (non-fatal)"
            ;;
        *)
            find "$PROJECT_ROOT/frontend/release" -maxdepth 1 -type f \( -name "zorai*" -o -name "zorai*" \) -delete 2>/dev/null || true
            npx electron-builder --win portable nsis || warn_msg "Electron Windows build failed (non-fatal)"
            ;;
    esac

    # Collect Electron artifacts
    RELEASE_DIR="$PROJECT_ROOT/frontend/release"
    if [[ -d "$RELEASE_DIR" ]]; then
        find "$RELEASE_DIR" -maxdepth 1 -type f \( -name "zorai*.exe" -o -name "zorai*.AppImage" -o -name "zorai*.deb" -o -name "zorai*.dmg" -o -name "zorai*.rpm" -o -name "zorai*.zip" \) 2>/dev/null | while read -r f; do
            cp "$f" "$OUT_DIR/"
            ok_msg "Electron: $(basename "$f")"
        done

        case "$OS" in
            Linux*)
                linux_appimage="$(find "$RELEASE_DIR" -maxdepth 1 -type f -name "zorai*.AppImage" -print -quit)"
                if [[ -n "$linux_appimage" ]]; then
                    cp "$linux_appimage" "$OUT_DIR/zorai-desktop"
                    chmod 755 "$OUT_DIR/zorai-desktop"
                    ok_msg "Electron launcher: zorai-desktop"
                fi
                ;;
            Darwin*)
                mac_app="$(find "$RELEASE_DIR" -maxdepth 2 -type d -name "zorai.app" -print -quit)"
                if [[ -n "$mac_app" ]]; then
                    rm -rf "$OUT_DIR/zorai-desktop.app"
                    cp -R "$mac_app" "$OUT_DIR/zorai-desktop.app"
                    cat > "$OUT_DIR/zorai-desktop" <<'SH'
#!/bin/sh
set -eu
dir="$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
app="$dir/zorai-desktop.app"
zip="$dir/zorai-desktop.app.zip"
if [ ! -x "$app/Contents/MacOS/zorai" ]; then
  rm -rf "$app"
  if command -v ditto >/dev/null 2>&1; then
    ditto -x -k "$zip" "$dir"
  else
    unzip -q "$zip" -d "$dir"
  fi
fi
exec "$app/Contents/MacOS/zorai" "$@"
SH
                    chmod 755 "$OUT_DIR/zorai-desktop"
                    (cd "$OUT_DIR" && ditto -c -k --sequesterRsrc --keepParent zorai-desktop.app zorai-desktop.app.zip)
                    rm -rf "$OUT_DIR/zorai-desktop.app"
                    ok_msg "Electron launcher: zorai-desktop + zorai-desktop.app.zip"
                else
                    warn_msg "Electron macOS app bundle was not produced under $RELEASE_DIR"
                fi
                ;;
            *)
                portable="$RELEASE_DIR/zorai-portable.exe"
                if [[ -f "$portable" ]]; then
                    cp "$portable" "$OUT_DIR/zorai-desktop.exe"
                    ok_msg "Electron launcher: zorai-desktop.exe"
                fi
                ;;
        esac
    fi
fi

# -----------------------------------------------------------
# Step 6: Package bundle + checksums + notes
# -----------------------------------------------------------
step_msg "Packaging release bundle..."

bundle_artifacts=()
for file in "$OUT_DIR"/*; do
    [[ -f "$file" ]] || continue
    base_name="$(basename "$file")"
    case "$base_name" in
        *.zip|SHA256SUMS*.txt|RELEASE_NOTES*.md) continue ;;
    esac
    bundle_artifacts+=("$base_name")
done

if [[ ${#bundle_artifacts[@]} -gt 0 ]]; then
    notes_file="$OUT_DIR/RELEASE_NOTES.md"
    case "$PLATFORM_DIR" in
        linux) artifact_os_name="linux" ;;
        macos) artifact_os_name="darwin" ;;
        windows) artifact_os_name="windows" ;;
        *)
            warn_msg "Unknown platform '$PLATFORM_DIR' for artifact naming; falling back to normalized OS slug"
            artifact_os_name="$(printf '%s' "${OS_SLUG:-$OS}" | tr '[:upper:]' '[:lower:]')"
            ;;
    esac
    artifact_arch_name="$ARCH"
    case "$TARGET" in
        x86_64-unknown-linux-gnu|x86_64-apple-darwin)
            artifact_arch_name="x86_64"
            ;;
        aarch64-unknown-linux-gnu|aarch64-apple-darwin)
            artifact_arch_name="aarch64"
            ;;
    esac
    if [[ "$artifact_os_name" == "windows" ]]; then
        case "$artifact_arch_name" in
            x86_64) artifact_arch_name="x64" ;;
            aarch64) artifact_arch_name="arm64" ;;
        esac
    fi
    checksums_file="$OUT_DIR/SHA256SUMS-${artifact_os_name}-${artifact_arch_name}.txt"
    bundle_file="$OUT_DIR/zorai-${artifact_os_name}-${artifact_arch_name}.zip"

    generate_release_notes_if_missing "$notes_file" "${bundle_artifacts[@]}"
    write_checksums_file "$checksums_file" "${bundle_artifacts[@]}"
    create_bundle_zip "$bundle_file" "${bundle_artifacts[@]}" "$(basename "$checksums_file")" "$(basename "$notes_file")"

    ok_msg "Created $(basename "$checksums_file")"
    ok_msg "Created $(basename "$notes_file")"
    ok_msg "Created $(basename "$bundle_file")"
else
    warn_msg "No release artifacts available to package"
fi

# -----------------------------------------------------------
# Summary
# -----------------------------------------------------------
echo ""
echo "============================================================"
echo " Build complete!"
echo "============================================================"
echo ""
echo "  Output: $OUT_DIR/"
echo ""
for f in "$OUT_DIR"/*; do
    [[ -f "$f" ]] || continue
    local_size=$(du -h "$f" | cut -f1)
    printf "  %-30s %s\n" "$(basename "$f")" "$local_size"
done
echo ""
if [[ $SIGN -eq 0 ]]; then
    echo "  Binaries are NOT signed. Run with --sign to sign."
fi
echo "============================================================"
