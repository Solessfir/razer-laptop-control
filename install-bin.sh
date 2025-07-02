#!/usr/bin/env bash

# GitHub repository details
REPO_OWNER="Solessfir"
REPO_NAME="razer-laptop-control"
LATEST_RELEASE_URL="https://api.github.com/repos/$REPO_OWNER/$REPO_NAME/releases/latest"

# Installation directories
BIN_DIR="/usr/bin"
SHARE_DIR="/usr/share/razercontrol"
APPLICATIONS_DIR="/usr/share/applications"
RULES_DIR="/etc/udev/rules.d"
SYSTEMD_USER_DIR="/etc/systemd/user"
OPENRC_DIR="/etc/init.d"

# Detect init system
detect_init_system() {
    if command -v systemctl &>/dev/null && systemctl --user list-jobs &>/dev/null; then
        INIT_SYSTEM="systemd"
    elif [ -f "/sbin/rc-update" ]; then
        INIT_SYSTEM="openrc"
    else
        INIT_SYSTEM="other"
    fi
}

# Download latest release
download_latest_release() {
    echo "Finding latest release..." >&2
    RELEASE_DATA=$(curl -s "$LATEST_RELEASE_URL")
    TARBALL_URL=$(echo "$RELEASE_DATA" | grep -E 'browser_download_url.*x86_64\.tar\.xz"' | cut -d '"' -f 4)
    VERSION=$(echo "$RELEASE_DATA" | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')
    
    if [ -z "$TARBALL_URL" ]; then
        echo "Error: Could not find release download URL" >&2
        exit 1
    fi

    DOWNLOAD_FILE="rlc-$VERSION-x86_64.tar.xz"
    echo "Downloading $DOWNLOAD_FILE..." >&2
    curl -L -o "$DOWNLOAD_FILE" "$TARBALL_URL" >&2
    
    # Extract to temporary directory
    EXTRACT_DIR=$(mktemp -d)
    echo "Extracting to $EXTRACT_DIR..." >&2
    tar -xf "$DOWNLOAD_FILE" -C "$EXTRACT_DIR" >&2
    
    # Find the content directory
    CONTENT_DIR=$(find "$EXTRACT_DIR" -mindepth 1 -maxdepth 1 -type d | head -n 1)
    
    if [ -z "$CONTENT_DIR" ]; then
        echo "Error: Could not find content directory in the archive" >&2
        exit 1
    fi
    
    echo "Files extracted to $CONTENT_DIR" >&2
    echo "$CONTENT_DIR"
}

# Stop service if running
stop_service() {
    echo "Stopping service if running..." >&2
    case $INIT_SYSTEM in
        systemd)
            systemctl --user stop razercontrol.service 2>/dev/null || true
            ;;
        openrc)
            sudo rc-service razercontrol stop 2>/dev/null || true
            ;;
    esac
}

# Install files
install_files() {
    local content_dir="$1"
    echo "Installing files from $content_dir..." >&2
    
    # Create directories
    sudo mkdir -p "$SHARE_DIR"
    sudo mkdir -p "$RULES_DIR"
    mkdir -p "$HOME/.local/share/razercontrol"
    
    # Install binaries and data files
    sudo install -m755 "$content_dir/razer-cli" "$BIN_DIR" || echo "Failed to install razer-cli" >&2
    sudo install -m755 "$content_dir/razer-settings" "$BIN_DIR" || echo "Failed to install razer-settings" >&2
    sudo install -m755 "$content_dir/daemon" "$SHARE_DIR" || echo "Failed to install daemon" >&2
    sudo install -m644 "$content_dir/laptops.json" "$SHARE_DIR" || echo "Failed to install laptops.json" >&2
    sudo install -m644 "$content_dir/99-hidraw-permissions.rules" "$RULES_DIR" || echo "Failed to install udev rules" >&2
    
    # Install desktop file if desktop environment exists
    if [ -d "/usr/share/applications" ] && [ -d "$HOME/.config" ]; then
        if [ -f "$content_dir/razer-settings.desktop" ]; then
            sudo install -m644 "$content_dir/razer-settings.desktop" "$APPLICATIONS_DIR"
        else
            echo "Warning: Desktop file not found" >&2
        fi
    fi
    
    # Install service files
    case $INIT_SYSTEM in
        systemd)
            sudo mkdir -p "$SYSTEMD_USER_DIR"
            if [ -f "$content_dir/services/systemd/razercontrol.service" ]; then
                sudo install -m644 "$content_dir/services/systemd/razercontrol.service" "$SYSTEMD_USER_DIR"
                
                # Correct service file permissions
                sudo chmod 644 "$SYSTEMD_USER_DIR/razercontrol.service"
                
                # Update service file to point to correct daemon path
                sudo sed -i "s|ExecStart=.*|ExecStart=$SHARE_DIR/daemon|" "$SYSTEMD_USER_DIR/razercontrol.service"
            else
                echo "Warning: systemd service file not found - using default" >&2
                # Create minimal systemd service file
                echo "[Unit]
Description=Razer Control Daemon
After=network.target

[Service]
Type=simple
ExecStart=$SHARE_DIR/daemon
Restart=on-failure
RestartSec=5
Environment=\"RUST_BACKTRACE=1\"

[Install]
WantedBy=default.target" | sudo tee "$SYSTEMD_USER_DIR/razercontrol.service" >/dev/null
            fi
            ;;
        openrc)
            if [ -f "$content_dir/services/openrc/razercontrol" ]; then
                sudo install -m755 "$content_dir/services/openrc/razercontrol" "$OPENRC_DIR"
                # Update username and daemon path in service file
                sudo sed -i "s/USERNAME_CHANGEME/$USER/" "$OPENRC_DIR/razercontrol"
                sudo sed -i "s|/usr/share/razercontrol/daemon|$SHARE_DIR/daemon|" "$OPENRC_DIR/razercontrol"
            else
                echo "Warning: OpenRC service file not found - using default" >&2
                # Create minimal OpenRC service file
                echo "#!/sbin/openrc-run
description=\"Razer Control Daemon\"

command=\"$SHARE_DIR/daemon\"
command_args=\"\"
command_user=\"$USER\"
pidfile=\"/run/razercontrol.pid\"

depend() {
    need localmount
    after udev
}

start() {
    ebegin \"Starting Razer Control\"
    start-stop-daemon --start --exec \$command --user \$command_user
    eend \$?
}

stop() {
    ebegin \"Stopping Razer Control\"
    start-stop-daemon --stop --exec \$command --user \$command_user
    eend \$?
}" | sudo tee "$OPENRC_DIR/razercontrol" >/dev/null
                sudo chmod +x "$OPENRC_DIR/razercontrol"
            fi
            ;;
    esac
    
    # Reload udev rules
    echo "Reloading udev rules..." >&2
    sudo udevadm control --reload-rules
    sudo udevadm trigger
}

# Start service
start_service() {
    echo "Starting service..." >&2
    case $INIT_SYSTEM in
        systemd)
            systemctl --user daemon-reload
            if [ -f "$SYSTEMD_USER_DIR/razercontrol.service" ]; then
                systemctl --user enable --now razercontrol.service || {
                    echo "Failed to start via systemd. Trying manual start..." >&2
                    nohup "$SHARE_DIR/daemon" >/dev/null 2>&1 &
                    sleep 2
                    if pgrep -f "$SHARE_DIR/daemon" >/dev/null; then
                        echo "Daemon started manually" >&2
                    else
                        echo "Failed to start daemon manually" >&2
                    fi
                }
            else
                echo "Error: systemd service file not installed" >&2
            fi
            ;;
        openrc)
            if [ -f "$OPENRC_DIR/razercontrol" ]; then
                sudo rc-update add razercontrol default
                sudo rc-service razercontrol start
            else
                echo "Error: OpenRC service file not installed" >&2
            fi
            ;;
    esac
    
    # Verify daemon is running
    sleep 2
    if pgrep -f "$SHARE_DIR/daemon" >/dev/null; then
        echo "Daemon is running" >&2
    else
        echo "Warning: Daemon is not running! Trying to start manually..." >&2
        nohup "$SHARE_DIR/daemon" >/dev/null 2>&1 &
        sleep 2
        if pgrep -f "$SHARE_DIR/daemon" >/dev/null; then
            echo "Daemon started manually" >&2
        else
            echo "Failed to start daemon" >&2
            echo "Try running manually with: $SHARE_DIR/daemon" >&2
            echo "Check logs with: journalctl --user-unit=razercontrol.service" >&2
        fi
    fi
}

# Verify installation
verify_installation() {
    echo "Verifying installation..." >&2
    
    # Check binaries
    for binary in razer-cli razer-settings; do
        if ! command -v "$binary" &> /dev/null; then
            echo "Error: $binary not found in PATH" >&2
        else
            echo "$binary installed: $(command -v "$binary")" >&2
        fi
    done
    
    # Check daemon process
    if pgrep -f "$SHARE_DIR/daemon" >/dev/null; then
        echo "Daemon running with PID: $(pgrep -f "$SHARE_DIR/daemon")" >&2
    else
        echo "Warning: Daemon process not found" >&2
    fi
    
    # Check service file
    case $INIT_SYSTEM in
        systemd)
            if [ -f "$SYSTEMD_USER_DIR/razercontrol.service" ]; then
                echo "Systemd service installed: $SYSTEMD_USER_DIR/razercontrol.service" >&2
            else
                echo "Warning: Systemd service file missing" >&2
            fi
            ;;
        openrc)
            if [ -f "$OPENRC_DIR/razercontrol" ]; then
                echo "OpenRC service installed: $OPENRC_DIR/razercontrol" >&2
            else
                echo "Warning: OpenRC service file missing" >&2
            fi
            ;;
    esac
    
    # Test basic functionality with a safe command
    if command -v razer-cli &> /dev/null; then
        echo "Testing connection to daemon:" >&2
        # Use a safe read command to verify communication
        razer-cli read sync >/dev/null 2>&1
        if [ $? -eq 0 ]; then
            echo "Successfully communicated with daemon" >&2
        else
            echo "Warning: Failed to communicate with daemon" >&2
            echo "Check daemon status with: systemctl --user status razercontrol" >&2
            echo "View daemon logs with: journalctl --user-unit=razercontrol.service" >&2
        fi
    fi
}

# Uninstall function
uninstall() {
    echo "Uninstalling Razer Control..." >&2
    
    stop_service
    
    # Remove files
    sudo rm -f "$BIN_DIR/razer-cli"
    sudo rm -f "$BIN_DIR/razer-settings"
    sudo rm -f "$APPLICATIONS_DIR/razer-settings.desktop"
    sudo rm -f "$SHARE_DIR/daemon"
    sudo rm -f "$SHARE_DIR/laptops.json"
    sudo rm -f "$RULES_DIR/99-hidraw-permissions.rules"
    
    # Remove service files
    case $INIT_SYSTEM in
        systemd)
            sudo rm -f "$SYSTEMD_USER_DIR/razercontrol.service"
            systemctl --user daemon-reload
            ;;
        openrc)
            sudo rc-update del razercontrol default
            sudo rm -f "$OPENRC_DIR/razercontrol"
            ;;
    esac
    
    # Remove user data
    rm -rf "$HOME/.config/razercontrol" 2>/dev/null
    rm -rf "$HOME/.local/share/razercontrol" 2>/dev/null
    
    # Reload udev rules
    sudo udevadm control --reload-rules
    sudo udevadm trigger
    
    echo "Razer Control uninstalled" >&2
}

# Cleanup function
cleanup() {
    # Remove downloaded archive
    if [ -f "rlc-"*".tar.xz" ]; then
        echo "Removing downloaded archive..." >&2
        rm -f rlc-*.tar.xz
    fi
    
    # Remove temporary directory
    if [ -n "$EXTRACT_DIR" ] && [ -d "$EXTRACT_DIR" ]; then
        echo "Removing temporary files..." >&2
        rm -rf "$EXTRACT_DIR"
    fi
}

# Main function
main() {
    # Set up trap to ensure cleanup runs on exit
    trap cleanup EXIT
    
    if [ "$(id -u)" -eq 0 ]; then
        echo "Please run as regular user (not root)" >&2
        exit 1
    fi

    detect_init_system
    
    if [ "$INIT_SYSTEM" = "other" ]; then
        echo "Unsupported init system" >&2
        exit 1
    fi

    case $1 in
        install)
            CONTENT_DIR=$(download_latest_release)
            stop_service
            install_files "$CONTENT_DIR"
            start_service
            verify_installation
            echo "Installation complete" >&2
            echo "Note: You may need to log out and back in for the service to start" >&2
            echo "You can verify the daemon is running with: pgrep -af daemon" >&2
            ;;
        uninstall)
            uninstall
            ;;
        *)
            echo "Usage: $0 {install|uninstall}" >&2
            exit 1
            ;;
    esac
}

main "$@"