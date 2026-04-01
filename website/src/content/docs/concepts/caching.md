---
title: Caching
description: How Vorpal uses content-addressed storage to cache builds and avoid redundant work.
---

Vorpal's caching system is built on a simple idea: if the inputs have not changed, the output does not need to be rebuilt. Every artifact is identified by a SHA-256 hash of its inputs, and cached outputs are stored by that hash. This page explains how the caching layers work together to make builds fast.

## How content addressing enables caching

When you define an artifact, Vorpal serializes its entire definition -- name, sources, build steps, environment variables, dependencies -- into JSON and computes a SHA-256 digest. This digest is the artifact's identity.

Because the digest is derived from all inputs, two important things are always true:

1. **Same inputs = same digest.** If nothing has changed in your artifact definition or its sources, the digest is the same as last time.
2. **Different inputs = different digest.** If you change anything -- a source file, an environment variable, a dependency version -- the digest changes.

This means Vorpal never needs to decide whether a cache entry is still valid. If the digest matches, the cached output is correct by construction. If the digest does not match, the artifact needs to be rebuilt. There is no TTL, no heuristic invalidation, and no manual cache-busting.

## Cache lookup order

When you run `vorpal build`, each artifact goes through a three-level cache lookup before Vorpal resorts to building from scratch:

### 1. Local output cache

Vorpal first checks if the artifact output already exists on the local filesystem at `/var/lib/vorpal/`. If the output directory for the artifact's digest is present, the build is skipped entirely. This is the fastest path -- no network calls, no service communication, just a filesystem check.

This is why second builds are nearly instant: all outputs from the first build are already present locally.

### 2. Registry cache

If the output is not available locally, Vorpal checks the Registry (either local or remote). The Agent queries the Archive Service to see if a cached archive exists for this digest. If found, Vorpal downloads and unpacks the archive, skipping the build.

This layer is what enables cache sharing across machines. A CI server that has already built an artifact can push the output to a shared Registry. When a developer builds the same artifact, they pull the cached output instead of rebuilding.

### 3. Full build

If neither cache has the artifact, Vorpal builds it from scratch: the Agent prepares sources, the Worker executes build steps, and the output is archived and stored in the Registry for future cache hits.

## What triggers a cache miss

A cache miss happens when the artifact's content digest changes. Since the digest is derived from all inputs, any of the following changes will cause a rebuild:

- **Source file changes** -- Adding, modifying, or removing files that are included in a source
- **Build step changes** -- Modifying the script, arguments, entrypoint, or environment variables of any build step
- **Dependency changes** -- If artifact A depends on artifact B, and B's digest changes, then A's digest changes too (because B's digest is part of A's step definition)
- **Target system changes** -- Changing the list of target platforms
- **Lockfile changes** -- Unlocking sources and re-resolving them to different digests

Changes that do **not** trigger a cache miss:

- **File timestamps** -- Vorpal normalizes all file timestamps to the Unix epoch (January 1, 1970) before hashing. This means that touching a file without changing its content does not invalidate the cache.
- **Build environment differences** -- Because the digest is computed from the artifact definition (not the build machine's state), the same artifact built on different machines produces the same digest.

## Source caching

Sources have their own caching layer. When the Agent resolves sources, it computes a content digest for each source's files. These digests are recorded in `Vorpal.lock`.

On subsequent builds, the Agent checks the lockfile first. If a source's lockfile entry matches, the Agent skips downloading and hashing entirely -- it reuses the locked digest. This provides significant speedup for projects with large or remote sources, because the most expensive part of source resolution (downloading and hashing) is avoided.

For HTTP sources, the Agent also maintains an in-memory cache within a session. If multiple artifacts reference the same URL, the download happens once and is shared across all consumers.

## Archive compression

Cached artifacts are stored as zstd-compressed tar archives. Zstd was chosen for its balance of compression ratio and speed -- it compresses and decompresses significantly faster than gzip while producing smaller archives. This keeps both storage costs and transfer times low.

## Cache management

Vorpal does not automatically evict cached artifacts. Outputs accumulate in `/var/lib/vorpal/` until you explicitly clean them up using `vorpal system prune` with one or more flags:

```bash
# Remove everything (archives, outputs, configs, aliases, sandboxes)
vorpal system prune --all

# Remove specific cache types
vorpal system prune --artifact-archives --artifact-outputs
```

Running `vorpal system prune` without any flags does nothing. The available flags are:

- `--all` -- remove all cached resources
- `--artifact-aliases` -- remove artifact aliases
- `--artifact-archives` -- remove artifact archives
- `--artifact-configs` -- remove artifact build configs
- `--artifact-outputs` -- remove artifact outputs
- `--sandboxes` -- remove sandbox directories

## Sharing caches across machines

The Registry is the mechanism for sharing cached artifacts. In a team setup:

1. A CI server builds artifacts and pushes outputs to a shared Registry (backed by S3 or a shared filesystem)
2. Developers' local Vorpal instances are configured to check the shared Registry
3. When a developer runs `vorpal build`, artifacts that CI has already built are pulled from the Registry instead of being rebuilt locally

This works reliably because of content addressing: the same inputs always produce the same digest, so a cache entry created by CI is guaranteed to be the correct output for a developer building the same artifact.
