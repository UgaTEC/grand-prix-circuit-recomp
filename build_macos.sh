#!/bin/bash
# ==============================================================================
#  Build script for Grand Prix Circuit Native Recompilation on macOS (Apple Silicon / Intel)
# ==============================================================================
set -e

echo "=== Grand Prix Circuit - macOS Build ==="

# Check Rust / Cargo
if ! command -v cargo &> /dev/null; then
    echo "[ERROR] Cargo/Rust is not installed. Install via: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

ARCH=$(uname -m)
echo "[INFO] Detected architecture: $ARCH"

# Build release binary
echo "[INFO] Building release binary with Cargo..."
cargo build --release --manifest-path gprix_recomp/Cargo.toml

# Prepare distribution folder
OUT_DIR="GrandPrixCircuit_macOS"
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
