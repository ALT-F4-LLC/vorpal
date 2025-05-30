---
title: Installing Vorpal
description: A guide telling you how to go about installing Vorpal.
---

This guide will walk you through installing Vorpal.

## Prerequisites

- Bubblewrap
- Docker and the `buildx` plugin

## Setting up the `/var/lib/vorpal` directory

Vorpal requires that you have a directory (`/var/lib/vorpal`) created ahead of
time with a specific structure so it can persist built binaries and build
sources to the disk.

You can set this filesystem structure up with three simple commands (though
you'll need root, so either `sudo su` beforehand or prefix each command with
`sudo`!)

```shell
mkdir -pv /var/lib/vorpal/{key,sandbox,store}
mkdir -pv /var/lib/vorpal/store/artifact/{alias,archive,config,output}
chown -R $(id -u):$(id -g) /var/lib/vorpal
```

## Install

Now you've correctly set up your `/var/lib/vorpal` directory, you can install
the Vorpal binary itself and set up the cryptographic keys used for builds.

### From a binary release

Vorpal distributes a nightly binary release over on GitHub. To install it to
`/usr/local/bin`, you can run the following:

```bash
export VORPAL_VERSION="nightly"
export VORPAL_ARCH="x86_64" # either x86_64 or aarch64
export VORPAL_PLATFORM="linux" # either linux or darwin
curl -L "https://github.com/ALT-F4-LLC/vorpal/releases/download/$VORPAL_VERSION/vorpal-${VORPAL_ARCH}-${VORPAL_PLATFORM}.tar.gz" -o /tmp/vorpal.tar.gz
tar -xf /tmp/vorpal.tar.gz -C /usr/local/bin
rm vorpal.tar.gz
```

### From source

TODO
