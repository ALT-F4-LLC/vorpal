---
title: Check, Fix, Verify (specimen)
description: Draft specimen of the troubleshooting cause blocks; not built for production.
summary: Draft specimen of the cause blocks, from the troubleshooting template. Not built for production.
kind: troubleshooting
draft: true
---

A draft page (`draft: true`), shown only by `astro dev`, so the cause blocks can be checked before the Troubleshooting pages are written (redesign step 6). The text is from the "Find out why an artifact rebuilt" template.

## Causes, most likely first

### A file in `includes` changed {#cause-files}

Generated files and lockfiles count. File timestamps do not: Vorpal resets them before hashing.

#### Check

Run this before and after the change, or on both machines, and compare. The first line that differs is where the change entered.

```bash prompt
vorpal build my-app --list
```

```text output
cpython  <digest>
uv       <digest>
my-app   <digest>
```

#### Fix

Compare that artifact's definitions, then narrow its `includes` if the change is one you don't want to track.

```bash prompt
vorpal build example --export > before.json
# make or undo the change
vorpal build example --export > after.json
diff before.json after.json
```

#### Verify

Build twice. The second build prints the same digest and shows no step output.

### A different SDK version {#cause-sdk}

Built-in tools pin their versions and download URLs, and on Linux every shell step depends on `linux-vorpal`, so an SDK upgrade usually changes every digest.

#### Check

Compare the `vorpal-sdk` version in each machine's lockfile (`uv.lock`, `bun.lock`, `go.sum` or `Cargo.lock`).

#### Fix

Use the same SDK version on every machine that shares a cache.

#### Verify

`vorpal build my-app --list` prints the same digests on both machines.
