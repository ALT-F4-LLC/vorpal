---
slug: v/0.6/getting-started/installation
title: Installation
description: Install Vorpal on macOS or Linux and get the background services running.
---

Vorpal provides a single install script that downloads a Vorpal release (`0.6.0` unless you choose another version), generates TLS keys for secure communication between components, and starts the background services.

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

#### CLI flags

The installer accepts the following flags. When piping through `curl`, pass flags after `bash -s --`:

| Flag | Description |
|------|-------------|
| `-y`, `--yes` | Run in non-interactive mode (skip prompts) |
| `-v`, `--version <ver>` | Version to install (default: `0.6.0`) |
| `--services <list>` | Comma-separated services to install (default: `agent,registry,worker`) |
| `--no-service` | Skip service installation |
| `--no-path` | Skip PATH configuration |
| `--dry-run` | Show what would be done without making changes |
| `--uninstall` | Uninstall Vorpal |
| `--issuer <url>` | OIDC issuer URL, passed to the services as `--issuer` |
| `--issuer-audience <value>` | OIDC issuer audience, passed to the services as `--issuer-audience` |
| `--issuer-client-id <value>` | OIDC client ID, passed to the services as `--issuer-client-id` |
| `--issuer-client-secret <value>` | OIDC client secret for the services (see below) |
| `-h`, `--help` | Show the help message |

For example, to install only the agent and worker services:

```bash
curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash -s -- --services agent,worker
```

To install a specific version without starting any services:

```bash
curl -fsSL https://raw.githubusercontent.com/ALT-F4-LLC/vorpal/main/script/install.sh | bash -s -- --version 0.6.0 --no-service
```

#### Environment variables

As an alternative, the installer also accepts environment variables. These are useful when piping through `curl` without `bash -s --`:

| Variable | Effect |
|----------|--------|
| `VORPAL_NONINTERACTIVE=1` | Enable non-interactive mode |
| `CI=true` | Enable non-interactive mode |
| `VORPAL_VERSION=<ver>` | Version to install (default: `0.6.0`) |
| `VORPAL_SERVICES=<list>` | Comma-separated services to install (default: `agent,registry,worker`) |
| `VORPAL_NO_SERVICE=1` | Skip service installation |
| `VORPAL_NO_PATH=1` | Skip PATH configuration |
| `VORPAL_DRY_RUN=1` | Show what would be done without making changes |
| `VORPAL_ISSUER=<url>` | OIDC issuer URL |
| `VORPAL_ISSUER_AUDIENCE=<value>` | OIDC issuer audience |
| `VORPAL_ISSUER_CLIENT_ID=<value>` | OIDC client ID |
| `VORPAL_ISSUER_CLIENT_SECRET=<value>` | OIDC client secret |
| `NO_COLOR=1` | Disable color output |

Note that `CI` and `NO_COLOR` are environment-variable-only options with no flag equivalents. Conversely, `--uninstall` and `--help` are flag-only with no environment variable equivalents.

The installer adds each issuer option you set to the service definition it writes (the LaunchAgent on macOS, the systemd unit on Linux). If the installed binary supports `--issuer-client-secret-file`, the installer writes the client secret to `/var/lib/vorpal/key/issuer-client-secret` (mode `0600`) and passes the file path, so the service definition does not contain the secret. Older binaries receive the secret value on the service command line. See [`vorpal system services start`](/reference/cli/system/services/) for what each option does.

For example, using environment variables:

```bash
VORPAL_SERVICES=agent,worker \
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
launchctl bootout gui/$(id -u)/com.altf4llc.vorpal
launchctl bootstrap gui/$(id -u) ~/Library/LaunchAgents/com.altf4llc.vorpal.plist

# Linux
systemctl --user restart vorpal.service
```

### Building from source

If you need to build Vorpal from source instead of using the install script, see the <a href="https://github.com/ALT-F4-LLC/vorpal/blob/main/README.md#contributing" target="_blank">contributing guide</a> for build instructions.

## Next steps

With Vorpal installed, head to the [Quickstart](/getting-started/quickstart/) to create and build your first project.
