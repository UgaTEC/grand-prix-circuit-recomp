# Grand Prix Circuit (1988) - Native 64-bit Recompilation

[![CI/CD Multi-Platform Build](https://github.com/OWNER/REPO/actions/workflows/build_and_release.yml/badge.svg)](https://github.com/OWNER/REPO/actions/workflows/build_and_release.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/Platform-Windows%20%7C%20Linux%20%7C%20macOS-blue.svg)](https://github.com/OWNER/REPO/releases)

A modern, standalone, native 64-bit recompilation and enhanced port of the legendary Formula 1 simulator **Grand Prix Circuit** (Accolade / Distinctive Software, 1988).

Written in **Rust**, this port runs natively on modern operating systems without emulation layers like DOSBox, adding true **60 FPS scanline motion interpolation**, real-time **Commodore 64 (VIC-II / SID 6581) mode switching**, and multi-platform support.

---

## Key Features

- **Native 64-bit Port**: Built in Rust for Windows (x64), Linux (x64 / Steam Deck), and macOS (Apple Silicon M-Series & Intel).
- **60 FPS Spatial Motion Interpolation (`F9`)**:
  - Eliminates the original 1988 engine's 20 FPS judder using **2D scanline optical flow displacement**.
  - Curves, kerbs, and road lines physically translate across the screen at 60 FPS with **zero motion blur, zero ghosting, and zero transparency**.
  - Dashboard and cockpit remain 100% stationary, sharp, and vibration-free.
  - Press **`F9`** in real time to toggle between `[ 60 FPS INTERPOLATION ]` and `[ 20 FPS CLASSIC ]`.
- **Integrated Commodore 64 Mode (`F10`)**:
  - Hot-toggle between **MS-DOS** (EGA 16-color + PC Speaker audio) and **Commodore 64** (VIC-II palette + MOS 6581 SID sound synthesis).
  - Cycle-accurate MOS 6502 CPU + MOS 6581 SID emulator playing Kris Hatlelid's iconic C64 soundtrack.
- **In-Memory EXEPACK Support**:
  - Automatically depacks the commercial retail DOS executable (`GPEGA.EXE`) directly in memory. No external unpacker or Python scripts required.
- **Low Latency & High Performance**:
  - Optimized CPU execution budget completely eliminates frame drops in crowded corners.

---

## Legal Notice / Disclaimer

This project is a clean-room source port and reverse-engineering effort created strictly for interoperability, modern system compatibility, and historical preservation.

**This repository does NOT contain any copyrighted game binaries, images, tracks, sounds, or disk images.**
To play the game, you must provide the game data files from your own legally purchased copy of *Grand Prix Circuit* (e.g. from original 5.25"/3.5" floppy disks or a digital backup).

---

## How to Play

### 1. Download Pre-Built Release
Download the archive for your operating system from the [Releases](https://github.com/OWNER/REPO/releases) tab:
- **Windows**: `GrandPrixCircuit_Windows_x64.zip`
- **Linux / Steam Deck**: `GrandPrixCircuit_Linux_x64.tar.gz`
- **macOS (Apple Silicon M1/M2/M3/M4)**: `GrandPrixCircuit_macOS_Silicon.zip`
- **macOS (Intel)**: `GrandPrixCircuit_macOS_Intel.zip`

Extract the contents into a folder on your computer.

### 2. Supply Your Original Game Files
Copy the following files from your original MS-DOS game into the extracted folder (or inside a subfolder named `gprix2/`):
- `GPEGA.EXE` (or `GPEGA_UNP.EXE`)
- All `*.PES` and `*.PCS` track/car assets (e.g. `STATUS.PES`, `CAR1.PES`, `DETROIT.PES`, `MONACO.PES`, etc.)
- *(Optional, for C64 Mode audio)*: `Grand_Prix_Circuit.sid`

### 3. Launch & Controls
Run `GrandPrixCircuit.exe` (Windows) or `./GrandPrixCircuit` (Linux / macOS).

| Key | Action |
|---|---|
| **`F9`** | Toggle **60 FPS Motion Interpolation** / **20 FPS Classic** |
| **`F10`** | Toggle **MS-DOS Mode** (EGA/Speaker) / **Commodore 64 Mode** (VIC-II/SID) |
| **`Up Arrow` / `NumPad 8`** | Accelerate |
| **`Down Arrow` / `NumPad 2`** | Brake |
| **`Left` / `Right Arrow`** | Steer |
| **`Space` / `Left Shift`** | Shift Up |
| **`Enter` / `Left Ctrl`** | Shift Down |
| **`Escape`** | Pause Game / Return to Menu |

---

## Building from Source

### Prerequisites
- [Rust toolchain](https://rustup.rs/) (edition 2021, stable)

#### Linux Build Dependencies
On Debian, Ubuntu, or SteamOS:
```bash
sudo apt-get update && sudo apt-get install -y libasound2-dev libx11-dev libxcursor-dev pkg-config
```
On Fedora / RHEL:
```bash
sudo dnf install -y alsa-lib-devel libX11-devel pkgconf-pkg-config
```
On Arch Linux:
```bash
sudo pacman -S --noconfirm alsa-lib libx11 pkgconf
```

### Compiling
```bash
# Clone the repository
git clone https://github.com/OWNER/REPO.git
cd REPO

# Build in release mode
cargo build --release --manifest-path gprix_recomp/Cargo.toml

# Or use the platform helper scripts:
./build_linux.sh   # Linux
./build_macos.sh   # macOS
```
The compiled binary will be located at `gprix_recomp/target/release/gprix_recomp` (or `gprix_recomp.exe` on Windows).

---

## License

This project is licensed under the [MIT License](LICENSE).  
Grand Prix Circuit is a registered trademark / copyright of Accolade / Distinctive Software (1988).
