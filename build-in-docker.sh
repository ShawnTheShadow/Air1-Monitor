#!/bin/bash
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CONTAINER_NAME="air1-monitor-build"

echo "Building Air1 Monitor package in Arch Linux container..."

# Run build in Arch Linux container
docker run --rm \
    --name "$CONTAINER_NAME" \
    -v "$SCRIPT_DIR:/pkg" \
    archlinux:latest \
    bash -c '
        set -e
        echo "==> Updating system and installing build dependencies..."
        pacman -Syu --noconfirm
        pacman -S --noconfirm base-devel rust cargo
        
        echo "==> Creating build user (makepkg cannot run as root)..."
        useradd -m builder
        
        echo "==> Copying source to builder home..."
        cp -r /pkg /home/builder/build
        chown -R builder:builder /home/builder/build
        
        echo "==> Building package..."
        su - builder -c "cd /home/builder/build && makepkg -sf --noconfirm"
        
        echo "==> Copying package back..."
        cp /home/builder/build/*.pkg.tar.zst /pkg/ || true
        chown $(stat -c "%u:%g" /pkg) /pkg/*.pkg.tar.zst 2>/dev/null || true
    '

echo ""
echo "==> Build complete!"
echo "Package files:"
ls -lh "$SCRIPT_DIR"/*.pkg.tar.zst 2>/dev/null || echo "No package files found"
