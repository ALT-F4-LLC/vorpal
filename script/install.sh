#!/bin/bash
set -euo pipefail

## TODO: Support installing only specific components (registry, worker, etc.)

# Non-interactive mode: set via VORPAL_NONINTERACTIVE=1, CI=true, or -y/--yes flag
NONINTERACTIVE="${VORPAL_NONINTERACTIVE:-${CI:-0}}"

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        -y|--yes)
            NONINTERACTIVE=1
            shift
            ;;
        -h|--help)
            echo "Usage: install.sh [OPTIONS]"
            echo ""
            echo "Install Vorpal to ~/.vorpal and configure system services."
            echo ""
            echo "Options:"
            echo "  -y, --yes     Run in non-interactive mode (skip prompts)"
            echo "  -h, --help    Show this help message"
            echo ""
            echo "Environment variables:"
            echo "  VORPAL_NONINTERACTIVE=1    Enable non-interactive mode"
            echo "  CI=true                    Enable non-interactive mode"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Environment variables
INSTALL_ARCH=$(uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/')
INSTALL_DIR="$HOME/.vorpal"
INSTALL_OS=$(uname -s | tr '[:upper:]' '[:lower:]')
INSTALL_VERSION="nightly"
INSTALL_BINARY_URL="https://github.com/ALT-F4-LLC/vorpal/releases/download/$INSTALL_VERSION/vorpal-$INSTALL_ARCH-$INSTALL_OS.tar.gz"

if [[ "$NONINTERACTIVE" != "1" && "$NONINTERACTIVE" != "true" ]]; then
    read -p "|> Install script requires sudo permissions. Would you like to continue? (y/n) " -n 1 -r

    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo -e "\nAborting."
        exit 1
    fi
else
    echo "|> Install script requires sudo permissions. Continuing (non-interactive mode)..."
fi

if [ -d "$INSTALL_DIR" ]; then
    if [[ "$NONINTERACTIVE" != "1" && "$NONINTERACTIVE" != "true" ]]; then
        echo -e ""
        read -p "|> Install path $INSTALL_DIR exists. Would you like to replace? (y/n) " -n 1 -r

        if [[ ! $REPLY =~ ^[Yy]$ ]]; then
            echo -e "\nAborting."
            exit 1
        fi
    else
        echo "|> Install path $INSTALL_DIR exists. Replacing (non-interactive mode)..."
    fi

    rm -rf "$INSTALL_DIR"
fi

echo -e "\n|> Installing in $INSTALL_DIR directory..."

# Setup installation directories
mkdir -p "$INSTALL_DIR/bin"

# Download and extract the Vorpal binary
curl -s -L "$INSTALL_BINARY_URL" | tar xz -C "$INSTALL_DIR/bin"

# Setup directories
sudo mkdir -pv /var/lib/vorpal/{key,log,sandbox,store}
sudo mkdir -pv /var/lib/vorpal/store/artifact/{alias,archive,config,output}
sudo chown -R "$(id -u):$(id -g)" /var/lib/vorpal

# Generate a new keypair
echo -e "|> Generating a new keypair..."
"$INSTALL_DIR/bin/vorpal" system keys generate

# Setup LaunchAgent for macOS
if [[ $INSTALL_OS == "darwin" ]]; then
echo -e "|> Setting up LaunchAgent for macOS..."

cat <<EOF > "$HOME/Library/LaunchAgents/com.altf4llc.vorpal.plist"
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN"
"http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <!-- Unique identifier for your LaunchAgent -->
    <key>Label</key>
    <string>com.altf4llc.vorpal</string>

    <!-- Path to your Rust binary -->
    <key>ProgramArguments</key>
    <array>
        <string>${INSTALL_DIR}/bin/vorpal</string>
        <string>start</string>
    </array>

    <key>StandardOutPath</key>
    <string>/var/lib/vorpal/log/services.log</string>

    <key>StandardErrorPath</key>
    <string>/var/lib/vorpal/log/services.log</string>

    <!-- Start on login/load -->
    <key>RunAtLoad</key>
    <true/>

    <!-- Keep the process alive (restart if it exits) -->
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
EOF

launchctl unload "$HOME/Library/LaunchAgents/com.altf4llc.vorpal.plist"
launchctl load "$HOME/Library/LaunchAgents/com.altf4llc.vorpal.plist"
fi

# Setup systemd service for Linux
if [[ $INSTALL_OS == "linux" ]]; then
echo -e "|> Setting up systemd service for Linux..."

cat <<EOF | sudo tee /etc/systemd/system/vorpal.service
[Unit]
Description=Vorpal
After=network.target

[Service]
Type=simple
ExecStart=${INSTALL_DIR}/bin/vorpal services start
Restart=always
RestartSec=5

[Install]
WantedBy=multi-user.target
EOF

sudo systemctl daemon-reload
sudo systemctl enable vorpal.service
sudo systemctl start vorpal.service
fi

echo -e "|> Vorpal installed and started."
