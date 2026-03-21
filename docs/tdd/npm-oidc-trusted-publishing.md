# TDD: Migrate `release-sdk-typescript` to NPM OIDC Trusted Publishing

## 1. Problem Statement

The `release-sdk-typescript` job in `.github/workflows/vorpal.yaml` (lines 467-494) currently
publishes the `@altf4llc/vorpal-sdk` package to NPM using a legacy access token (`NPM_TOKEN`
secret) passed as `NODE_AUTH_TOKEN`. This approach has several drawbacks:

- **Secret rotation burden**: The `NPM_TOKEN` must be manually generated, stored in GitHub
  Secrets, and periodically rotated.
- **Broad blast radius**: A leaked token grants publish access until revoked, with no audit trail
  tying publishes back to specific CI runs.
- **No provenance attestation**: The current `npm publish --tag next` command does not include the
  `--provenance` flag, so published packages lack a verifiable link back to the source commit and
  CI run that produced them.
- **Inconsistency with binary release job**: The `release` job (lines 291-341) already uses
  `id-token: write` and `actions/attest-build-provenance@v4` for binary artifacts, but the
  TypeScript SDK publish does not follow the same trust model.

NPM trusted publishing with OIDC (GA as of July 2025) eliminates long-lived tokens entirely.
GitHub Actions requests a short-lived OIDC token scoped to the specific workflow run, and NPM
verifies it against a pre-configured trusted publisher policy on the package.

### Constraints

- The NPM package `@altf4llc/vorpal-sdk` must have a trusted publisher configured on npmjs.com
  before the workflow change is deployed.
- The migration must not break the existing release process; a failed publish on a tag push is
  disruptive.
- The `--tag next` behavior must be preserved.

### Success Criteria

1. The `release-sdk-typescript` job publishes without any long-lived NPM secret.
2. Published packages include OIDC provenance attestation (verifiable via `npm audit signatures`).
3. The `NPM_TOKEN` secret can be deleted from the repository after migration.
4. No regression in publish behavior (correct tag, correct package contents).

---

## 2. Context & Prior Art

### Current Implementation (lines 467-494 of `.github/workflows/vorpal.yaml`)

```yaml
release-sdk-typescript:
  if: ${{ github.event_name == 'push' && contains(github.ref, 'refs/tags/') && !contains(github.ref, 'nightly') }}
  needs:
    - test
  permissions:
    contents: read
    id-token: write          # Already present but unused by npm publish
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6

    - uses: actions/setup-node@v4
      with:
        node-version: "22"
        registry-url: "https://registry.npmjs.org"

    - uses: oven-sh/setup-bun@v2

    - run: bun install
      working-directory: sdk/typescript

    - run: bun run build
      working-directory: sdk/typescript

    - run: npm publish --tag next
      working-directory: sdk/typescript
      env:
        NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}
```

### Specific Findings

| # | Finding | Severity | Line(s) |
|---|---------|----------|---------|
| 1 | `NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}` is a legacy access token pattern. OIDC trusted publishing does not use `NODE_AUTH_TOKEN` at all; the npm CLI obtains a short-lived token via the OIDC exchange automatically. | **Blocker** | 493-494 |
| 2 | `id-token: write` permission is already declared (line 473) but is not actually consumed by any step. This is the correct permission for OIDC, so it just needs the publish step to actually use it. | Concern | 473 |
| 3 | `npm publish --tag next` is missing the `--provenance` flag. Without `--provenance`, npm does not request an OIDC token and does not attach a Sigstore attestation to the published package. | **Blocker** | 491 |
| 4 | The `registry-url` in `actions/setup-node` is set to `https://registry.npmjs.org`, which is correct for OIDC trusted publishing. No change needed here. | Good | 481 |
| 5 | `publishConfig.access: "public"` is set in `sdk/typescript/package.json` (line 34-36). This is required for scoped packages and is correctly configured. | Good | package.json:34 |

### How NPM OIDC Trusted Publishing Works

Reference: [NPM Trusted Publishing with OIDC](https://github.blog/changelog/2025-07-31-npm-trusted-publishing-with-oidc-is-generally-available/)

1. The package owner configures a **trusted publisher** on npmjs.com, linking the package to a
   specific GitHub repository, workflow file, and (optionally) environment.
2. During CI, `npm publish --provenance` triggers the npm CLI to:
   a. Request an OIDC token from GitHub Actions (requires `id-token: write`).
   b. Exchange the OIDC token with npm's token endpoint for a short-lived publish token.
   c. Publish the package with a Sigstore provenance attestation.
3. No `NODE_AUTH_TOKEN` or `NPM_TOKEN` secret is needed.

### Precedent in This Repo

The `release` job (lines 291-341) already follows the OIDC pattern for binary attestation:
- `id-token: write` permission
- `actions/attest-build-provenance@v4`

The `release-sdk-rust` job (lines 441-465) uses a legacy `CARGO_REGISTRY_TOKEN` secret. Crates.io
does not yet support OIDC trusted publishing, so this is expected. Note: crates.io trusted
publishing support is tracked upstream and should be adopted when available.

---

## 3. Architecture & System Design

No architectural changes are needed. This is a configuration-level change to the existing workflow.

The component topology is unchanged:

```
Tag push -> GitHub Actions -> npm publish -> npmjs.com registry
```

The only difference is the authentication mechanism:

```
Before: GitHub Secret (NPM_TOKEN) -> NODE_AUTH_TOKEN env var -> npm CLI -> npmjs.com
After:  GitHub OIDC token -> npm CLI --provenance -> npmjs.com OIDC verification -> publish
```

---

## 4. Migration & Rollout Strategy

### Pre-requisites (Manual, Out-of-Band)

Before merging the workflow change, an npm package maintainer for `@altf4llc/vorpal-sdk` must:

1. Log in to [npmjs.com](https://www.npmjs.com/).
2. Navigate to the `@altf4llc/vorpal-sdk` package settings.
3. Under **Publishing access** (or **Trusted Publishers**), add a new trusted publisher:
   - **Repository owner**: `ALT-F4-LLC`
   - **Repository name**: `vorpal`
   - **Workflow filename**: `vorpal.yaml`
   - **Environment**: (leave blank unless the job uses a GitHub environment)
4. Verify the configuration is saved.

### Phased Rollout

**Phase 1: Configure trusted publisher on npmjs.com** (manual, prerequisite)

- Complexity: Small
- Risk: None (additive configuration, does not affect existing token-based publishes)

**Phase 2: Update the workflow** (code change)

The `release-sdk-typescript` job should be changed to:

```yaml
release-sdk-typescript:
  if: ${{ github.event_name == 'push' && contains(github.ref, 'refs/tags/') && !contains(github.ref, 'nightly') }}
  needs:
    - test
  permissions:
    contents: read
    id-token: write
  runs-on: ubuntu-latest
  steps:
    - uses: actions/checkout@v6

    - uses: actions/setup-node@v4
      with:
        node-version: "22"
        registry-url: "https://registry.npmjs.org"

    - uses: oven-sh/setup-bun@v2

    - run: bun install
      working-directory: sdk/typescript

    - run: bun run build
      working-directory: sdk/typescript

    - run: npm publish --tag next --provenance
      working-directory: sdk/typescript
```

Changes:
1. **Add `--provenance`** to `npm publish` command.
2. **Remove the `env: NODE_AUTH_TOKEN` block** entirely. The npm CLI will authenticate via OIDC.

- Complexity: Small
- Risk: Medium (if the trusted publisher is not configured first, the publish will fail)

**Phase 3: Clean up the legacy secret** (manual, post-verification)

- After verifying that a tag push successfully publishes via OIDC, delete the `NPM_TOKEN` secret
  from the repository's GitHub Settings.
- Revoke the token on npmjs.com.

- Complexity: Small
- Risk: Low (only after confirmed working)

### Rollback Plan

If the OIDC publish fails on a tag push:

1. Re-add `NODE_AUTH_TOKEN: ${{ secrets.NPM_TOKEN }}` to the publish step and remove `--provenance`.
2. Push the fix to main and re-tag (or manually trigger the workflow).
3. The `NPM_TOKEN` secret should be kept in the repository until at least one successful OIDC
   publish is confirmed.

---

## 5. Risks & Open Questions

| Risk | Likelihood | Impact | Mitigation |
|------|-----------|--------|------------|
| Trusted publisher not configured before workflow merge | Medium | Publish fails on next tag | Phase 1 must complete before Phase 2 merges. Document as a PR prerequisite. |
| npm CLI version on `ubuntu-latest` does not support `--provenance` | Low | Publish fails | Node 22 ships with npm 10+, which supports `--provenance`. The `actions/setup-node@v4` step ensures a modern npm. |
| npmjs.com OIDC service outage | Low | Publish fails for that run | Retry the workflow. Short-lived outages are recoverable. |
| `actions/setup-node` `.npmrc` template incompatible with OIDC | Low | Auth fails | The `registry-url` parameter generates a standard `.npmrc` that works with OIDC. Well-tested path. |

### Open Questions

1. **Should the job use a GitHub Environment?** Environments provide an additional layer of
   approval (manual gates) and can be specified in the trusted publisher configuration for
   stricter scoping. Currently no environment is used. This is optional but worth considering
   for production release flows.

2. **Should `--tag next` be dynamic?** Currently all tag pushes (excluding nightly) publish with
   `--tag next`. If stable releases should get `--tag latest`, the tagging logic may need
   adjustment. This is out of scope for the OIDC migration but worth noting.

---

## 6. Testing Strategy

- **Pre-merge**: There is no way to test OIDC publishing without pushing a real tag. Verify the
  trusted publisher configuration manually on npmjs.com before merging.
- **Smoke test**: After merging, push a tag (e.g., a pre-release like `v0.1.0-alpha.1`) and
  verify:
  1. The `release-sdk-typescript` job succeeds.
  2. The published package on npmjs.com shows a provenance badge.
  3. `npm audit signatures` passes for the published version.
- **Regression**: Verify the package contents are identical (same files, same `dist/` output) by
  comparing with a previous publish.

---

## 7. Implementation Phases

| Phase | Description | Depends On | Complexity |
|-------|-------------|------------|------------|
| 1 | Configure trusted publisher on npmjs.com for `@altf4llc/vorpal-sdk` | None | Small |
| 2 | Update workflow: add `--provenance`, remove `NODE_AUTH_TOKEN` | Phase 1 | Small |
| 3 | Verify publish on next tag push | Phase 2 | Small |
| 4 | Delete `NPM_TOKEN` secret and revoke token on npmjs.com | Phase 3 | Small |
