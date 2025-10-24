#!/usr/bin/env bash

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Default values
TARGET="all"
OUTPUT_DIR="releases"

# Print colored output
print_info() {
    echo -e "${BLUE}$1${NC}"
}

print_success() {
    echo -e "${GREEN}$1${NC}"
}

print_warning() {
    echo -e "${YELLOW}$1${NC}"
}

print_error() {
    echo -e "${RED}$1${NC}"
}

# Help message
show_help() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Build release binaries for server_users

OPTIONS:
    -t, --target <TARGET>     Target platform: macos, linux, or all (default: all)
    -o, --output <DIR>        Output directory for binaries (default: releases)
    -h, --help               Show this help message

EXAMPLES:
    $(basename "$0")                          # Build for all platforms
    $(basename "$0") --target macos           # Build for macOS only
    $(basename "$0") --target linux           # Build for Linux only
    $(basename "$0") -t macos -o dist         # Build for macOS, output to dist/

EOF
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--target)
            TARGET="$2"
            shift 2
            ;;
        -o|--output)
            OUTPUT_DIR="$2"
            shift 2
            ;;
        -h|--help)
            show_help
            exit 0
            ;;
        *)
            print_error "Unknown option: $1"
            show_help
            exit 1
            ;;
    esac
done

# Validate target
if [[ ! "$TARGET" =~ ^(macos|linux|all)$ ]]; then
    print_error "Invalid target: $TARGET"
    echo "Valid targets are: macos, linux, all"
    exit 1
fi

print_info "🔨 Building server_users releases..."

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Get version from Cargo.toml
VERSION=$(grep '^version' Cargo.toml | head -n1 | cut -d'"' -f2)
print_info "Version: $VERSION"

# Build for macOS ARM64
if [[ "$TARGET" == "macos" || "$TARGET" == "all" ]]; then
    echo ""
    print_info "📦 Building for macOS ARM64..."
    
    # Add target if not already added
    rustup target add aarch64-apple-darwin 2>/dev/null || true
    
    if cargo build --release --target aarch64-apple-darwin; then
        MACOS_BINARY="target/aarch64-apple-darwin/release/server_users"
        MACOS_OUTPUT="$OUTPUT_DIR/server_users-$VERSION-macos-arm64"
        
        if [[ -f "$MACOS_BINARY" ]]; then
            cp "$MACOS_BINARY" "$MACOS_OUTPUT"
            print_success "✅ macOS ARM64 binary created: $MACOS_OUTPUT"
            
            # Create tar.gz
            (cd "$OUTPUT_DIR" && tar czf "server_users-$VERSION-macos-arm64.tar.gz" "server_users-$VERSION-macos-arm64")
            print_success "✅ Archive created: $MACOS_OUTPUT.tar.gz"
            
            # Show file size
            SIZE=$(du -h "$MACOS_OUTPUT" | cut -f1)
            print_info "   Size: $SIZE"
        else
            print_error "❌ macOS build failed - binary not found"
        fi
    else
        print_error "❌ macOS build failed"
    fi
fi

# Build for Linux x86_64
if [[ "$TARGET" == "linux" || "$TARGET" == "all" ]]; then
    echo ""
    print_info "📦 Building for Linux x86_64..."
    
    # Check if cross is available for cross-compilation
    if command -v cross &> /dev/null; then
        print_info "Using 'cross' for cross-compilation..."
        BUILD_CMD="cross"
    else
        print_warning "⚠️  'cross' not found. Install with: cargo install cross"
        print_info "Attempting native cargo build (will only work on Linux)..."
        BUILD_CMD="cargo"
        
        # Add target if not already added
        rustup target add x86_64-unknown-linux-gnu 2>/dev/null || true
    fi
    
    if $BUILD_CMD build --release --target x86_64-unknown-linux-gnu; then
        LINUX_BINARY="target/x86_64-unknown-linux-gnu/release/server_users"
        LINUX_OUTPUT="$OUTPUT_DIR/server_users-$VERSION-linux-x86_64"
        
        if [[ -f "$LINUX_BINARY" ]]; then
            cp "$LINUX_BINARY" "$LINUX_OUTPUT"
            print_success "✅ Linux x86_64 binary created: $LINUX_OUTPUT"
            
            # Create tar.gz
            (cd "$OUTPUT_DIR" && tar czf "server_users-$VERSION-linux-x86_64.tar.gz" "server_users-$VERSION-linux-x86_64")
            print_success "✅ Archive created: $LINUX_OUTPUT.tar.gz"
            
            # Show file size
            SIZE=$(du -h "$LINUX_OUTPUT" | cut -f1)
            print_info "   Size: $SIZE"
        else
            print_error "❌ Linux build failed - binary not found"
        fi
    else
        print_error "❌ Linux build failed"
    fi
fi

echo ""
print_success "✨ Build process complete!"
echo ""
print_info "📁 Binaries location: $OUTPUT_DIR/"

# List created files
if command -v ls &> /dev/null; then
    ls -lh "$OUTPUT_DIR" | tail -n +2
fi
