#!/bin/bash
set -euo pipefail

# Environment variables
INSTALL_ARCH=$(uname -m | tr '[:upper:]' '[:lower:]' | sed 's/arm64/aarch64/')
INSTALL_DIR="$HOME/.vorpal"
INSTALL_OS=$(uname -s | tr '[:upper:]' '[:lower:]')
INSTALL_VERSION="nightly"
INSTALL_BINARY_URL="https://github.com/ALT-F4-LLC/vorpal/releases/download/$INSTALL_VERSION/vorpal-$INSTALL_ARCH-$INSTALL_OS.tar.gz"

read -p "|> Install script requires sudo permissions. Would you like to continue? (y/n) " -n 1 -r

if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo -e "\nAborting."
    exit 1
fi

if [ -d "$INSTALL_DIR" ]; then
    echo -e ""
    read -p "|> Install path $INSTALL_DIR exists. Would you like to replace? (y/n) " -n 1 -r

    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo -e "\nAborting."
        exit 1
    fi

    rm -rf "$INSTALL_DIR"
fi

echo -e "\n|> Installing in $INSTALL_DIR directory..."

# Setup installation directories
mkdir -p "$INSTALL_DIR/bin"

# Download and extract the Vorpal binary
curl -s -L "$INSTALL_BINARY_URL" | tar xz -C "$INSTALL_DIR/bin"

# Setup directories
sudo mkdir -pv /var/lib/vorpal/{cache,key,sandbox,store}
sudo chown -R "$(id -u):$(id -g)" /var/lib/vorpal

# Generate a new keypair
echo -e "|> Generating a new keypair..."
"$INSTALL_DIR/bin/vorpal" keys generate

# Setup LaunchAgent for macOS
if [[ $INSTALL_OS == "darwin" ]]; then
echo -e "|> Setting up LaunchAgent for macOS..."

cat <<EOF > "com.altf4llc.vorpal.plist"
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

    <!-- Start on login/load -->
    <key>RunAtLoad</key>
    <true/>

    <!-- Keep the process alive (restart if it exits) -->
    <key>KeepAlive</key>
    <true/>
</dict>
</plist>
EOF

# Move the plist file to the LaunchAgents directory
mv com.altf4llc.vorpal.plist ~/Library/LaunchAgents/
chmod 644 ~/Library/LaunchAgents/com.altf4llc.vorpal.plist

echo -e "|> Starting Vorpal service..."

launchctl unload "$HOME/Library/LaunchAgents/com.altf4llc.vorpal.plist"
launchctl load "$HOME/Library/LaunchAgents/com.altf4llc.vorpal.plist"

echo -e "|> Vorpal service(s) started"
fi

## TODO: implement Linux service setup
