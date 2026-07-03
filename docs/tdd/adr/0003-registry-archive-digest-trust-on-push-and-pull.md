---
project: "vorpal"
last_updated: "2026-07-02"
updated_by: "draft (pending @security-engineer acceptance)"
status: "proposed"
---

# ADR 0003: Registry archive digest trust on push and pull

## Status

**Proposed.** Drafted to record a confirmed gap surfaced by DKT-62. The tracing
evidence below is established; the *decision* (which mitigation to adopt) is
presented as a recommendation and requires `@security-engineer` acceptance before
it is treated as binding. No fix is implemented pending that acceptance — this
ADR exists so the confirmed finding is not lost (the issue explicitly forbids
implementing a fix before the decision is recorded).

## Context

The artifact registry serves built artifact archives by content digest. A push
stores bytes under a key derived from a digest; a pull fetches bytes by that same
key. Content-addressing is only as trustworthy as the link between *the bytes
stored* and *the digest that keys them*.

DKT-62 (re-filed from the closed DKT-55) asked whether the registry re-verifies
that link on either path. **Tracing confirms it does not, on either backend, in
either direction.** The digest is trusted verbatim from the caller and used
directly as the storage key, with no server-side re-hash on push and no
re-verification of fetched bytes on pull.

### Evidence — push path (no server-side re-hash)

The gRPC service layer trusts the client-supplied digest verbatim and forwards
it to the backend as the content key:

- `cli/src/command/start/registry.rs:289-314` — `ArchiveService::push` reads
  `request_digest = first_chunk.digest` from the *first* client chunk, validates
  only that it is non-empty, then calls `self.backend.push(&request_digest,
  &request_namespace, &mut data_stream)`. There is no accumulation and hashing
  of `data_stream` to compare against `request_digest` anywhere in the handler.
- `cli/src/command/start/registry/archive/s3.rs:80-92` — `S3Backend::push`
  derives `archive_key = get_artifact_archive_key(digest, namespace)` from the
  caller-supplied digest and **short-circuits as idempotent** if an object
  already exists at that key. The streamed bytes are uploaded under that key
  with no re-hash before or after the idempotency check.
- `cli/src/command/start/registry/archive/local.rs:59-64` — `LocalBackend::push`
  writes the stream to `get_artifact_archive_path(digest, namespace)` and
  **short-circuits as idempotent** if that path already exists. No re-hash of
  the written bytes against the supplied digest.

The sole producer today is the trusted worker, which computes the digest itself
(`cli/src/command/start/worker.rs:866,872,882-886` sends `digest: artifact_digest`
client-side alongside the data). In a fully trusted cluster this is internally
consistent. The gap is that **the registry does not independently enforce** the
content-addressed invariant: a write-capable client (a compromised worker, a
stolen/abused registry credential, or any caller with `push` permission) can
store arbitrary bytes under any chosen digest key, and that mismatch will never
be caught on the push path.

### Evidence — pull path (no re-verification)

Both backends fetch by the derived key and stream bytes back without re-hashing
them against the requested digest:

- `cli/src/command/start/registry/archive/s3.rs:42-70` — `S3Backend::pull` does
  `head_object` + `get_object` by the key derived from `request.digest`, then
  streams `archive_stream` chunks to the caller. No digest is recomputed over
  the served bytes.
- `cli/src/command/start/registry/archive/local.rs:27-51` — `LocalBackend::pull`
  reads `get_artifact_archive_path(&request.digest, …)` and streams its contents
  in chunks. No recomputation of the digest over the served bytes.
- Consumers `cli/src/command/start/worker.rs:173,291` and
  `cli/src/command/start/agent.rs:129,377,456` call `pull`/`check` keyed by the
  digest they hold and consume the returned stream; none re-hashes the fetched
  bytes against the digest before use (the registry is trusted as the source of
  truth).

`check` (head-only) is unaffected — it only tests key existence.

### Net effect

Because neither direction re-verifies the content↔digest link, a mismatched
push is **not caught downstream**: once bytes are stored under a wrong/malicious
digest key, every pull of that key serves the wrong bytes as if they matched,
with no layer raising an alarm. This is a registry-integrity / supply-chain
concern, not a fail-closed-gate bypass (the agent's `--unlock`/changed-source
gates in `agent.rs` are a separate trust path and remain intact).

## Decision

_(Recommended, pending `@security-engineer` acceptance.)_

**Adopt push-path server-side re-hash as the primary mitigation, and add a
pull-path re-verify as defense-in-depth.**

1. **Push-path re-hash (primary, closes the gap at the source of truth).** In
   `ArchiveService::push` (`registry.rs:289-314`), accumulate the data stream
   while forwarding it to the backend, compute the registry's canonical digest
   over the accumulated bytes, and **reject the push** if it does not equal the
   client-supplied `request_digest`. This makes the registry the authority on
   the content↔digest binding rather than trusting the caller. The digest
   algorithm must match the one the worker uses to produce `artifact_digest`
   (confirm the exact construction in `store/hashes.rs` / the worker build path
   before implementing).

2. **Pull-path re-verify (defense-in-depth, optional).** Optionally re-hash the
   served bytes in `pull` and fail the stream if they diverge from the requested
   digest. This catches a store whose bytes were mutated out-of-band (operator
   error, storage corruption, a path that bypassed the push re-hash such as a
   direct S3/local write). It is secondary because a correct push re-hash
   already prevents a mismatched key from ever being committed.

Rejected as the primary mitigation:

- **Accept with rationale only (no code change).** Rejected as the primary
  choice: the gap is a real integrity invariant the registry claims
  (content-addressed storage) but does not enforce. Documenting an unenforced
  invariant leaves the supply-chain exposure open. (This option remains valid if
  `@security-engineer` judges the trusted-cluster assumption strong enough; see
  Alternatives.)
- **Pull-path re-verify alone.** Rejected as primary: it detects a mismatch
  only at consumption time and leaves wrong bytes committed in the store;
  push-path re-hash prevents the bad commit in the first place.

## Consequences

- **Easier / stronger:** the registry becomes the actual authority on the
  content↔digest binding it advertises. A compromised write-capable client can
  no longer poison a digest key silently; pulls of a known-good digest are
  guaranteed to serve the bytes that hash to it.
- **Cost:** push now pays a single full pass over the bytes (hashing) in
  addition to the upload. For the artifact sizes involved this is acceptable;
  the hash can be computed incrementally over the same stream already being
  forwarded, avoiding a second read or full buffering. Implementation must keep
  the streaming/multipart behavior of both backends intact.
- **Harder / risk:** the re-hash must use the **exact** digest construction the
  worker uses to mint `artifact_digest`, or legitimate pushes will be rejected.
  The implementing issue must pin the algorithm by reference to
  `store/hashes.rs` and add a parity test (push-then-pull round-trip asserting a
  mismatched-digest push is rejected and a correct one is served byte-identical).
- **Neutral:** `check` (head-only) is unchanged; the agent's fail-closed source
  gates are on a separate trust path and are not affected by this ADR.

## Alternatives Considered

- **Accept the gap with documented rationale, no code change.** Defensible only
  if the deployment model guarantees every writer is a trusted worker on a
  trusted network and registry credentials are never abused. Leaves the
  invariant unenforced; rejected as primary but remains a valid choice if
  `@security-engineer` accepts the trusted-cluster assumption as sufficient and
  the operator concurs.
- **Pull-path re-verify only.** Detects at consumption, leaves bad bytes
  committed; rejected as primary, retained as optional defense-in-depth (see
  Decision §2).
- **Mint the digest server-side and ignore the client value entirely.** Cleanest
  in principle, but a larger change to the push contract (the worker currently
  supplies the digest it computed and relies on). Out of scope for the
  "verify-then-fix" framing of DKT-62; can be revisited if the push contract is
  ever redesigned.

## Trace (all `ArchiveBackend` impls and consumers, for the implementing issue)

- **Backends:** `cli/src/command/start/registry/archive/s3.rs`
  (`check`/`pull`/`push`), `cli/src/command/start/registry/archive/local.rs`
  (`check`/`pull`/`push`). No other `ArchiveBackend` implementations exist
  (grep-confirmed).
- **Service layer:** `cli/src/command/start/registry.rs:202` (check),
  `:221-264` (pull), `:272-319` (push).
- **Consumers:** `cli/src/command/start/worker.rs:173,291` (pull),
  `:896` (push); `cli/src/command/start/agent.rs:129,377` (check), `:456` (push).
