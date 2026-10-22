#!/bin/bash
# ==============================================================================
#  Build script for Grand Prix Circuit Native Recompilation on Linux (Ubuntu, Debian, Fedora, Arch, Steam Deck)
# ==============================================================================
set -e

echo "=== Grand Prix Circuit - Linux Build ==="

# Check Rust / Cargo
if ! command -v cargo &> /dev/null; then
    echo "[ERROR] Cargo/Rust is not installed. Install via: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Detect package manager and ensure ALSA & X11 dev headers are present
if command -v apt-get &> /dev/null; then
    echo "[INFO] Debian/Ubuntu detected. Verifying development libraries (libasound2-dev libx11-dev)..."
    dpkg -s libasound2-dev libx11-dev pkg-config &> /dev/null || {
        echo "[INFO] Installing required packages..."
        sudo apt-get update && sudo apt-get install -y libasound2-dev libx11-dev pkg-config
    }
elif command -v dnf &> /dev/null; then
    echo "[INFO] Fedora/RHEL detected. Verifying development libraries (alsa-lib-devel libX11-devel)..."
    rpm -q alsa-lib-devel libX11-devel pkgconf-pkg-config &> /dev/null || {
        echo "[INFO] Installing required packages..."
        sudo dnf install -y alsa-lib-devel libX11-devel pkgconf-pkg-config
    }
elif command -v pacman &> /dev/null; then
    echo "[INFO] Arch Linux / SteamOS detected. Verifying development libraries (alsa-lib libx11)..."
    pacman -Qi alsa-lib libx11 pkgconf &> /dev/null || {
        echo "[INFO] Installing required packages..."
        sudo pacman -S --noconfirm alsa-lib libx11 pkgconf
    }
fi

# Build release binary
echo "[INFO] Building release binary with Cargo..."
cargo build --release --manifest-path gprix_recomp/Cargo.toml

# Prepare distribution folder
OUT_DIR="GrandPrixCircuit_Linux"
echo "[INFO] Packaging application into $OUT_DIR/..."
mkdir -p "$OUT_DIR/gprix2"

cp gprix_recomp/target/release/gprix_recomp "$OUT_DIR/GrandPrixCircuit"
chmod +x "$OUT_DIR/GrandPrixCircuit"

# Copy MS-DOS game binary and asset files if locally available
if [ -f "GPEGA_UNP.EXE" ]; then
    cp GPEGA_UNP.EXE "$OUT_DIR/"
elif [ -f "GPEGA.EXE" ]; then
    cp GPEGA.EXE "$OUT_DIR/"
fi

if [ -d "gprix2" ]; then
    cp -r gprix2/* "$OUT_DIR/gprix2/" 2>/dev/null || true
fi

if [ -f "GrandPrixCircuit_Release/README.txt" ]; then
    cp GrandPrixCircuit_Release/README.txt "$OUT_DIR/"
fi

if [ -f "LICENSE" ]; then
    cp LICENSE "$OUT_DIR/"
fi

echo "=============================================================================="
echo " [SUCCESS] Build complete!"
echo " To run the game, open Terminal and execute:"
echo "   cd $OUT_DIR && ./GrandPrixCircuit"
echo " (Remember: Press F10 during the game to toggle Commodore 64 / DOS mode)"
echo "=============================================================================="
