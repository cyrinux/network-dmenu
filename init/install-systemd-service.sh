#!/usr/bin/env bash
# Install network-dmenu systemd user service

set -euo pipefail

# Configuration
SERVICE_NAME="network-dmenu"
SYSTEMD_USER_DIR="$HOME/.config/systemd/user"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

log_info() {
    echo -e "${BLUE}[INFO]${NC} $*"
}

log_success() {
    echo -e "${GREEN}[SUCCESS]${NC} $*"
}

log_warning() {
    echo -e "${YELLOW}[WARNING]${NC} $*"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $*"
}

# Check if network-dmenu binary exists
check_binary() {
    local binary_path="$HOME/.local/bin/network-dmenu"
    
    if [[ ! -f "$binary_path" ]]; then
        log_error "network-dmenu binary not found at $binary_path"
        log_info "Please install network-dmenu first or update the ExecStart path in the service file"
        return 1
    fi
    
    log_success "Found network-dmenu binary at $binary_path"
}

# Create systemd user directory
create_systemd_dir() {
    mkdir -p "$SYSTEMD_USER_DIR"
    log_info "Created systemd user directory: $SYSTEMD_USER_DIR"
}

# Install service file
install_service() {
    local service_file="$SCRIPT_DIR/systemd/network-dmenu.service"
    local privileged_service_file="$SCRIPT_DIR/systemd/network-dmenu-privileged.service"
    local target_file="$SYSTEMD_USER_DIR/${SERVICE_NAME}.service"
    
    if [[ ! -f "$service_file" ]]; then
        log_error "Service file not found: $service_file"
        return 1
    fi
    
    # Ask user which version to install
    echo
    log_info "Choose service version to install:"
    echo "1) Standard (recommended)"
    echo "2) Privileged (if standard version fails with permission errors)"
    echo
    read -p "Enter choice (1 or 2): " choice
    
    case $choice in
        1)
            cp "$service_file" "$target_file"
            log_success "Installed standard service file"
            ;;
        2)
            if [[ -f "$privileged_service_file" ]]; then
                cp "$privileged_service_file" "$target_file"
                log_success "Installed privileged service file"
                log_warning "Privileged version grants more system access - use only if needed"
            else
                log_error "Privileged service file not found: $privileged_service_file"
                return 1
            fi
            ;;
        *)
            log_error "Invalid choice. Please run the script again."
            return 1
            ;;
    esac
}

# Reload systemd and enable service
setup_service() {
    log_info "Reloading systemd user daemon..."
    systemctl --user daemon-reload
    
    log_info "Enabling and starting $SERVICE_NAME service..."
    systemctl --user enable "$SERVICE_NAME.service"
    systemctl --user start "$SERVICE_NAME.service"
    
    # Check service status
    if systemctl --user is-active --quiet "$SERVICE_NAME.service"; then
        log_success "Service is running successfully!"
    else
        log_error "Service failed to start. Check status with:"
        echo "  systemctl --user status $SERVICE_NAME.service"
        echo "  journalctl --user -u $SERVICE_NAME.service -f"
        return 1
    fi
}

# Show service status
show_status() {
    echo
    log_info "Service status:"
    systemctl --user status "$SERVICE_NAME.service" --no-pager
    
    echo
    log_info "To manage the service:"
    echo "  Start:   systemctl --user start $SERVICE_NAME.service"
    echo "  Stop:    systemctl --user stop $SERVICE_NAME.service"
    echo "  Restart: systemctl --user restart $SERVICE_NAME.service"
    echo "  Disable: systemctl --user disable $SERVICE_NAME.service"
    echo "  Logs:    journalctl --user -u $SERVICE_NAME.service -f"
}

# Main installation process
main() {
    log_info "Installing network-dmenu systemd user service..."
    
    check_binary
    create_systemd_dir
    install_service
    setup_service
    show_status
    
    log_success "Installation completed successfully!"
}

# Handle script interruption
cleanup() {
    log_warning "Installation interrupted"
    exit 1
}

trap cleanup INT TERM

# Run main function
main "$@"