# Network-dmenu Makefile
# Provides convenient targets for building with eBPF support

.PHONY: all clean build build-ebpf build-all test clippy fmt install-deps help

# Default target
all: build-all

# Install eBPF development dependencies
install-deps:
	@echo "🔧 Installing eBPF development dependencies..."
	@./scripts/install-bpf-deps.sh

# Build only the eBPF program
build-ebpf:
	@echo "🔧 Building eBPF program..."
	cargo xtask build-ebpf

# Build userspace program without BPF
build:
	@echo "🔧 Building userspace program (no BPF)..."
	cargo build

# Build both eBPF and userspace programs
build-all:
	@echo "🔧 Building both eBPF and userspace programs..."
	cargo xtask build-all

# Build release versions
build-release:
	@echo "🔧 Building release versions..."
	cargo xtask build-all --release

# Run tests
test:
	@echo "🧪 Running tests..."
	cargo test
	@echo "🧪 Running tests with BPF features..."
	cargo test --features bpf

# Run clippy linter
clippy:
	@echo "🔍 Running clippy..."
	cargo clippy
	cargo clippy --features bpf
	cd network-monitor-ebpf && cargo clippy

# Format code
fmt:
	@echo "🎨 Formatting code..."
	cargo fmt
	cd network-monitor-ebpf && cargo fmt

# Clean build artifacts
clean:
	@echo "🧹 Cleaning build artifacts..."
	cargo clean
	cd network-monitor-ebpf && cargo clean
	rm -rf target/bpf/

# Check all configurations
check-all:
	@echo "✅ Checking all configurations..."
	cargo check
	cargo check --features bpf
	cargo check --all-features
	cd network-monitor-ebpf && cargo check

# Run the daemon with BPF support (requires root)
run-daemon:
	@echo "🚀 Running daemon with BPF support (requires root)..."
	@if [ "$$(id -u)" != "0" ]; then \
		echo "❌ This target requires root privileges. Run with sudo."; \
		exit 1; \
	fi
	cargo run --features bpf -- --daemon --log-level debug

# Show available targets
help:
	@echo "📋 Available targets:"
	@echo "  install-deps   - Install eBPF development dependencies"
	@echo "  build-ebpf     - Build only the eBPF program"
	@echo "  build          - Build userspace program (no BPF)"
	@echo "  build-all      - Build both eBPF and userspace programs"
	@echo "  build-release  - Build release versions"
	@echo "  test           - Run tests"
	@echo "  clippy         - Run clippy linter"
	@echo "  fmt            - Format code"
	@echo "  clean          - Clean build artifacts"
	@echo "  check-all      - Check all configurations"
	@echo "  run-daemon     - Run daemon with BPF support (requires root)"
	@echo "  help           - Show this help"
	@echo ""
	@echo "🔧 BPF Development Workflow:"
	@echo "  1. make install-deps    # Install eBPF dependencies"
	@echo "  2. make build-all       # Build eBPF + userspace"
	@echo "  3. sudo make run-daemon # Test with root privileges"