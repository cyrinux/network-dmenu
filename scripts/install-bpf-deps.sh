#!/bin/bash
set -e

echo "🔧 Installing eBPF development dependencies..."

# Check if running as root
if [[ $EUID -eq 0 ]]; then
   echo "❌ Please don't run this script as root"
   exit 1
fi

# Detect package manager and install dependencies
if command -v apt-get &> /dev/null; then
    echo "📦 Detected apt package manager (Debian/Ubuntu)"
    echo "Installing eBPF development packages..."
    sudo apt-get update
    sudo apt-get install -y \
        clang \
        llvm \
        libelf-dev \
        libbpf-dev \
        bpf-tools \
        linux-headers-$(uname -r) \
        build-essential \
        pkg-config
        
elif command -v dnf &> /dev/null; then
    echo "📦 Detected dnf package manager (Fedora/RHEL)"
    echo "Installing eBPF development packages..."
    sudo dnf install -y \
        clang \
        llvm \
        elfutils-libelf-devel \
        libbpf-devel \
        bpftool \
        kernel-headers \
        kernel-devel \
        gcc \
        pkg-config
        
elif command -v pacman &> /dev/null; then
    echo "📦 Detected pacman package manager (Arch Linux)"
    echo "Installing eBPF development packages..."
    sudo pacman -S --needed \
        clang \
        llvm \
        libelf \
        libbpf \
        bpf \
        linux-headers \
        base-devel
        
elif command -v zypper &> /dev/null; then
    echo "📦 Detected zypper package manager (openSUSE)"
    echo "Installing eBPF development packages..."
    sudo zypper install -y \
        clang \
        llvm \
        libelf-devel \
        libbpf-devel \
        kernel-default-devel \
        gcc \
        pkg-config
        
else
    echo "❌ Unsupported package manager. Please install manually:"
    echo "   - clang"
    echo "   - llvm" 
    echo "   - libelf development headers"
    echo "   - libbpf development headers"
    echo "   - kernel headers"
    echo "   - build-essential/gcc"
    echo "   - pkg-config"
    exit 1
fi

# Install bpf-linker if not present
if ! command -v bpf-linker &> /dev/null; then
    echo "🔗 Installing bpf-linker..."
    cargo install bpf-linker
fi

# Check Rust target for eBPF
echo "🦀 Adding Rust eBPF targets..."
rustup target add bpfel-unknown-none
rustup target add bpfeb-unknown-none

echo "✅ eBPF development environment setup complete!"
echo ""
echo "📋 Next steps:"
echo "   1. Build eBPF program: cargo xtask build-ebpf"
echo "   2. Build with BPF support: cargo build --features bpf"
echo "   3. Run with BPF support: sudo ./target/debug/network-dmenu --daemon"
echo ""
echo "⚠️  Note: eBPF programs require root privileges or CAP_SYS_ADMIN to load"