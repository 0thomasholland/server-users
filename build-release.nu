#!/usr/bin/env nu

# Build script for creating release binaries for macOS ARM and Linux

def main [
    --target: string = "all"  # Target platform: macos, linux, or all
    --output-dir: string = "releases"  # Output directory for binaries
] {
    print "🔨 Building server_users releases..."
    
    # Create output directory
    mkdir $output_dir
    
    # Get version from Cargo.toml
    let version = (open Cargo.toml | get package.version)
    print $"Version: ($version)"
    
    # Build for macOS ARM64
    if $target == "macos" or $target == "all" {
        print "\n📦 Building for macOS ARM64..."
        cargo build --release --target aarch64-apple-darwin
        
        let macos_binary = $"target/aarch64-apple-darwin/release/server_users"
        let macos_output = $"($output_dir)/server_users-($version)-macos-arm64"
        
        if ($macos_binary | path exists) {
            cp $macos_binary $macos_output
            print $"✅ macOS ARM64 binary created: ($macos_output)"
            
            # Create tar.gz
            tar czf $"($macos_output).tar.gz" -C $output_dir $"server_users-($version)-macos-arm64"
            print $"✅ Archive created: ($macos_output).tar.gz"
        } else {
            print "❌ macOS build failed - binary not found"
        }
    }
    
    # Build for Linux x86_64
    if $target == "linux" or $target == "all" {
        print "\n📦 Building for Linux x86_64..."
        
        # Check if cross is available for cross-compilation
        let has_cross = (which cross | length) > 0
        
        if $has_cross {
            cross build --release --target x86_64-unknown-linux-gnu
        } else {
            print "⚠️  'cross' not found. Install with: cargo install cross"
            print "Attempting native cargo build (will only work on Linux)..."
            cargo build --release --target x86_64-unknown-linux-gnu
        }
        
        let linux_binary = "target/x86_64-unknown-linux-gnu/release/server_users"
        let linux_output = $"($output_dir)/server_users-($version)-linux-x86_64"
        
        if ($linux_binary | path exists) {
            cp $linux_binary $linux_output
            print $"✅ Linux x86_64 binary created: ($linux_output)"
            
            # Create tar.gz
            tar czf $"($linux_output).tar.gz" -C $output_dir $"server_users-($version)-linux-x86_64"
            print $"✅ Archive created: ($linux_output).tar.gz"
        } else {
            print "❌ Linux build failed - binary not found"
        }
    }
    
    print "\n✨ Build process complete!"
    print $"\n📁 Binaries location: ($output_dir)/"
    ls $output_dir | select name size
}
