#!/bin/bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OUT_DIR="${SCRIPT_DIR}/out"
API_LEVEL="${API_LEVEL:-26}"

# ANSI Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
BOLD='\033[1m'
NC='\033[0m'

log_info()    { echo -e "${BLUE}[INFO]${NC} $*"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $*"; }
log_warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
log_error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }

show_help() {
    cat <<EOF
Usage: $(basename "$0") [COMMAND|PROFILE]

Commands:
  release    Build optimized release binary (default)
  debug      Build unoptimized debug binary with debug symbols
  clean      Remove build artifacts (target/ and out/)
  help       Show this help message

Environment variables:
  ANDROID_NDK_HOME  Path to Android NDK toolchain
  API_LEVEL         Target Android API level (default: 26)
  CARGO             Path to cargo binary
EOF
    exit 0
}

# Command dispatching
CMD="${1:-release}"
case "$CMD" in
    help|-h|--help)
        show_help
        ;;
    clean)
        log_info "Cleaning build artifacts..."
        rm -rf "${OUT_DIR}" "${SCRIPT_DIR}/target"
        log_success "Clean completed."
        exit 0
        ;;
    release|debug)
        PROFILE="$CMD"
        ;;
    *)
        log_error "Unknown argument: '$CMD'"
        show_help
        ;;
esac

# Locate Android NDK
find_ndk() {
    if [ -n "${ANDROID_NDK_HOME:-}" ] && [ -d "$ANDROID_NDK_HOME" ]; then
        echo "$ANDROID_NDK_HOME"
        return 0
    fi
    if [ -n "${ANDROID_NDK_ROOT:-}" ] && [ -d "$ANDROID_NDK_ROOT" ]; then
        echo "$ANDROID_NDK_ROOT"
        return 0
    fi
    local candidates=(
        "/home/rapli/android-ndk-r26b"
        "$HOME/android-ndk-r26b"
        "$HOME/Android/Sdk/ndk/"*
        "/opt/android-ndk"*
    )
    for c in "${candidates[@]}"; do
        if [ -d "$c" ]; then
            echo "$c"
            return 0
        fi
    done
    return 1
}

if ! ANDROID_NDK_HOME="$(find_ndk)"; then
    log_error "Android NDK not found. Please export ANDROID_NDK_HOME=/path/to/ndk"
    exit 1
fi

NDK_BIN="$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
CLANG="$NDK_BIN/aarch64-linux-android${API_LEVEL}-clang"
CLANGXX="$NDK_BIN/aarch64-linux-android${API_LEVEL}-clang++"
AR="$NDK_BIN/llvm-ar"
STRIP="$NDK_BIN/llvm-strip"

if [ ! -x "$CLANG" ]; then
    log_error "Clang compiler not found at: $CLANG"
    exit 1
fi

# Export NDK cross-compilation environment
export CC_aarch64_linux_android="$CLANG"
export CXX_aarch64_linux_android="$CLANGXX"
export AR_aarch64_linux_android="$AR"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CLANG"
export RUSTFLAGS="-C link-arg=-Wl,-export-dynamic"

# Locate Cargo
CARGO_BIN="${CARGO:-cargo}"
if ! command -v "$CARGO_BIN" &>/dev/null && [ -x "$HOME/.cargo/bin/cargo" ]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
fi

if ! command -v "$CARGO_BIN" &>/dev/null; then
    log_error "Cargo not found. Please install Rust toolchain."
    exit 1
fi

echo -e "${BOLD}=== Building Face AIDL Bridge for Transsion ($PROFILE, aarch64) ===${NC}"
log_info "Using NDK: $ANDROID_NDK_HOME (API $API_LEVEL)"
log_info "Using Cargo: $CARGO_BIN"

cd "$SCRIPT_DIR"

# Compile via Cargo
if [ "$PROFILE" = "release" ]; then
    "$CARGO_BIN" build --target aarch64-linux-android --release
    BIN="target/aarch64-linux-android/release/android-hardware-biometrics-face-service-transsion"
    log_info "Stripping release binary..."
    "$STRIP" "$BIN"
else
    "$CARGO_BIN" build --target aarch64-linux-android
    BIN="target/aarch64-linux-android/debug/android-hardware-biometrics-face-service-transsion"
fi

# Prepare output layout
log_info "Assembling output package in ${OUT_DIR}..."
rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR/vendor/bin/hw"
mkdir -p "$OUT_DIR/vendor/etc/init"
mkdir -p "$OUT_DIR/vendor/etc/vintf/manifest"

# Copy files and enforce standard permissions
cp -f "$BIN" "$OUT_DIR/vendor/bin/hw/android.hardware.biometrics.face-service.default"
chmod 0755 "$OUT_DIR/vendor/bin/hw/android.hardware.biometrics.face-service.default"

cp -f "${SCRIPT_DIR}/android.hardware.biometrics.face-service.default.rc" "$OUT_DIR/vendor/etc/init/"
chmod 0644 "$OUT_DIR/vendor/etc/init/android.hardware.biometrics.face-service.default.rc"

cp -f "${SCRIPT_DIR}/manifest_face_transsion.xml" "$OUT_DIR/vendor/etc/vintf/manifest/"
chmod 0644 "$OUT_DIR/vendor/etc/vintf/manifest/manifest_face_transsion.xml"

# Package archives
tar -czf "${OUT_DIR}/face_vendor_package.tar.gz" -C "${OUT_DIR}" vendor

if command -v zip &>/dev/null; then
    (cd "${OUT_DIR}" && zip -rq "face_vendor_package.zip" vendor)
else
    python3 -c '
import zipfile, os
os.chdir("'"${OUT_DIR}"'")
with zipfile.ZipFile("face_vendor_package.zip", "w", zipfile.ZIP_DEFLATED) as z:
    for root, _, files in os.walk("vendor"):
        for f in files:
            z.write(os.path.join(root, f))
'
fi

echo ""
log_success "Build completed successfully!"
echo -e "${BOLD}Output Files (${OUT_DIR}):${NC}"
ls -lh "${OUT_DIR}" | grep -E '\.(tar\.gz|zip)$' || true
echo ""
echo -e "${BOLD}Vendor Files Structure:${NC}"
find "${OUT_DIR}/vendor" -type f -exec ls -lh {} +
