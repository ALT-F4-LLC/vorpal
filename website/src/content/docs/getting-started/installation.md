---
title: Installation
description: Install Vorpal on macOS or Linux and get the background services running.
---

Vorpal provides a single install script that downloads the latest release, generates TLS keys for secure communication between components, and starts the background services.

## Requirements

Vorpal supports the following platforms:

| OS    | Architecture        |
|-------|---------------------|
| macOS | Apple Silicon (ARM64) |
| macOS | Intel (x86_64)      |
| Linux | x86_64              |
| Linux | ARM64               |

## Install

Run the install script:

```bash
curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash
```

The installer performs three steps:

1. **Downloads the Vorpal binary** to `~/.vorpal/bin/vorpal` and adds it to your `PATH`.
2. **Generates TLS security keys** using `vorpal system keys generate`. These keys secure communication between the Vorpal CLI and its background services.
3. **Installs and starts background services** that handle builds, caching, and artifact storage.

On macOS, services run via a LaunchAgent (`com.altf4llc.vorpal`). On Linux, services run via a systemd user unit (`vorpal.service`).

### Installer options

The installer accepts environment variables to customize behavior:

| Variable                | Effect                          |
|-------------------------|---------------------------------|
| `VORPAL_VERSION=<ver>`  | Install a specific version (default: `nightly`) |
| `VORPAL_NO_SERVICE=1`   | Skip service installation       |
| `VORPAL_NO_PATH=1`      | Skip PATH configuration         |
| `VORPAL_NONINTERACTIVE=1` | Run without prompts           |
| `VORPAL_DRY_RUN=1`      | Show what would be done without making changes |

For example, to install a specific version without starting services:

```bash
VORPAL_VERSION=v0.1.0 VORPAL_NO_SERVICE=1 \
  curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash
```

## Verify the installation

After installation, confirm Vorpal is available:

```bash
vorpal --version
```

Check that background services are running:

```bash
# macOS
launchctl list | grep vorpal

# Linux
systemctl --user status vorpal.service
```

## Troubleshooting

### Services failed to start

Check the service logs for error details:

```bash
# macOS
cat /var/lib/vorpal/log/services.log

# Linux
journalctl --user -u vorpal.service --no-pager -n 20
```

Common causes include port conflicts (another process using the Vorpal socket) and permission issues on `/var/lib/vorpal`.

To restart services manually:

```bash
# macOS
launchctl kickstart gui/$(id -u)/com.altf4llc.vorpal

# Linux
systemctl --user restart vorpal.service
```

### Building from source

If you need to build Vorpal from source instead of using the install script, see the [contributing guide](https://github.com/ALT-F4-LLC/vorpal/blob/main/docs/spec/operations.md) for build instructions.

## Next steps

With Vorpal installed, head to the [Quickstart](/getting-started/quickstart/) to create and build your first project.
