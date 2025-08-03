#!/bin/bash
# Cross-platform build script for comfy-fs

set -e

# Configuration
PROJECT_NAME="comfy-fs"
BUILD_DIR="target/release"
DIST_DIR="dist"

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log() {
    echo -e "${BLUE}[BUILD]${NC} $1"
}

success() {
    echo -e "${GREEN}[SUCCESS]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

# Clean previous builds
clean() {
    log "Cleaning previous builds..."
    rm -rf "$DIST_DIR"
    mkdir -p "$DIST_DIR"
    cargo clean
}

# Check if target is installed
check_target() {
    local target=$1
    if ! rustup target list --installed | grep -q "$target"; then
        log "Installing target: $target"
        rustup target add "$target"
    fi
}

# Build for specific target
build_target() {
    local target=$1
    local output_name=$2
    
    log "Building for target: $target"
    check_target "$target"
    
    # Build with optimizations
    RUSTFLAGS="-C target-cpu=native" cargo build \
        --release \
        --target "$target" \
        --bin "$PROJECT_NAME"
    
    # Copy binary to dist directory
    local binary_path="target/$target/release/$PROJECT_NAME"
    if [[ "$target" == *"windows"* ]]; then
        binary_path="${binary_path}.exe"
        output_name="${output_name}.exe"
    fi
    
    if [[ -f "$binary_path" ]]; then
        cp "$binary_path" "$DIST_DIR/$output_name"
        success "Built: $output_name"
    else
        error "Failed to build for $target - binary not found at $binary_path"
        return 1
    fi
}

# Build all targets
build_all() {
    log "Starting cross-platform build..."
    
    # Linux targets
    build_target "x86_64-unknown-linux-gnu" "comfy-fs-linux-x86_64"
    build_target "x86_64-unknown-linux-musl" "comfy-fs-linux-x86_64-musl"
    build_target "aarch64-unknown-linux-gnu" "comfy-fs-linux-arm64"
    
    # Windows targets
    if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
        build_target "x86_64-pc-windows-gnu" "comfy-fs-windows-x86_64"
    else
        warn "MinGW not found, skipping Windows builds"
    fi
    
    # macOS targets (only on macOS)
    if [[ "$OSTYPE" == "darwin"* ]]; then
        build_target "x86_64-apple-darwin" "comfy-fs-macos-intel"
        build_target "aarch64-apple-darwin" "comfy-fs-macos-arm64"
    else
        warn "Not on macOS, skipping macOS builds"
    fi
}

# Create checksums
create_checksums() {
    log "Creating checksums..."
    cd "$DIST_DIR"
    sha256sum * > checksums.sha256
    cd ..
    success "Checksums created"
}

# Package builds
package() {
    log "Creating packages..."
    cd "$DIST_DIR"
    
    for binary in comfy-fs-*; do
        if [[ -f "$binary" && "$binary" != "checksums.sha256" ]]; then
            tar -czf "${binary}.tar.gz" "$binary"
            log "Packaged: ${binary}.tar.gz"
        fi
    done
    
    cd ..
    success "All packages created"
}

# Show usage
usage() {
    echo "Usage: $0 [command]"
    echo ""
    echo "Commands:"
    echo "  clean     Clean previous builds"
    echo "  build     Build for current platform only"
    echo "  all       Build for all platforms"
    echo "  package   Package all builds"
    echo "  help      Show this help"
    echo ""
    echo "Example: $0 all"
}

# Main execution
main() {
    case "${1:-build}" in
        clean)
            clean
            ;;
        build)
            clean
            build_target "$(rustc -vV | sed -n 's|host: ||p')" "$PROJECT_NAME"
            ;;
        all)
            clean
            build_all
            create_checksums
            ;;
        package)
            package
            ;;
        help|--help|-h)
            usage
            ;;
        *)
            error "Unknown command: $1"
            usage
            exit 1
            ;;
    esac
}

main "$@"