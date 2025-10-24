.PHONY: help build-macos build-linux build-all release clean install-deps

# Default target
help:
	@echo "Available targets:"
	@echo "  build-macos    - Build for macOS ARM64"
	@echo "  build-linux    - Build for Linux x86_64 (requires cross)"
	@echo "  build-all      - Build for all platforms"
	@echo "  release        - Build optimized release binaries"
	@echo "  clean          - Clean build artifacts"
	@echo "  install-deps   - Install build dependencies"

# Build for macOS ARM64
build-macos:
	@echo "Building for macOS ARM64..."
	rustup target add aarch64-apple-darwin
	cargo build --release --target aarch64-apple-darwin
	@echo "Binary location: target/aarch64-apple-darwin/release/server_users"

# Build for Linux x86_64 (cross-compilation)
build-linux:
	@echo "Building for Linux x86_64..."
	@command -v cross >/dev/null 2>&1 || { echo "Installing cross..."; cargo install cross; }
	cross build --release --target x86_64-unknown-linux-gnu
	@echo "Binary location: target/x86_64-unknown-linux-gnu/release/server_users"

# Build for all platforms
build-all: build-macos build-linux

# Create release packages
release:
	@echo "Creating release packages..."
	@mkdir -p releases
	@if [ -f target/aarch64-apple-darwin/release/server_users ]; then \
		cp target/aarch64-apple-darwin/release/server_users releases/server_users-macos-arm64; \
		cd releases && tar czf server_users-macos-arm64.tar.gz server_users-macos-arm64; \
		echo "✓ Created releases/server_users-macos-arm64.tar.gz"; \
	fi
	@if [ -f target/x86_64-unknown-linux-gnu/release/server_users ]; then \
		cp target/x86_64-unknown-linux-gnu/release/server_users releases/server_users-linux-x86_64; \
		cd releases && tar czf server_users-linux-x86_64.tar.gz server_users-linux-x86_64; \
		echo "✓ Created releases/server_users-linux-x86_64.tar.gz"; \
	fi

# Clean build artifacts
clean:
	cargo clean
	rm -rf releases

# Install build dependencies
install-deps:
	@echo "Installing Rust targets..."
	rustup target add aarch64-apple-darwin
	rustup target add x86_64-unknown-linux-gnu
	@echo "Installing cross for cross-compilation..."
	cargo install cross --git https://github.com/cross-rs/cross
