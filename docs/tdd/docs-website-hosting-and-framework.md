---
project: "vorpal"
maturity: "stable"
last_updated: "2026-03-21"
updated_by: "@staff-engineer"
scope: "Hosting solution and framework selection for the Vorpal documentation website"
owner: "@staff-engineer"
dependencies:
  - ../spec/architecture.md
  - ../spec/operations.md
---

# Documentation Website: Hosting and Framework Selection

## 1. Problem Statement

Vorpal is approaching its first stable release (v0.1.0) and currently has no dedicated documentation website. All documentation lives in markdown files within the repository (`README.md`, `docs/spec/*.md`) and inline CLI help (`vorpal --help`). This is insufficient for a project seeking adoption by external developers who need:

- Getting-started guides and tutorials
- SDK reference documentation for Rust, Go, and TypeScript
- Conceptual architecture explanations
- Configuration reference
- Migration guides as the project evolves

**Why now:** The v0.1.0 release marks Vorpal's transition from internal/experimental to a publicly usable build system. Without a documentation website, the barrier to adoption is high -- developers must read source code and scattered markdown files to understand the system.

**Constraints:**

- The project is open-source (Apache 2.0), hosted on GitHub under the `ALT-F4-LLC` organization
- The team is small; operational overhead for docs infrastructure must be minimal
- Documentation content is markdown-first (existing `docs/spec/` files, README)
- The project has SDKs in three languages (Rust, Go, TypeScript) requiring multi-language code examples
- CI/CD is already on GitHub Actions
- No existing custom domain is required at launch (can use a platform-provided subdomain initially)

**Acceptance Criteria:**

1. A hosting solution is selected that supports static site deployment with zero ongoing infrastructure management
2. The hosting solution provides automatic per-PR preview deployments for documentation review
3. A documentation framework is selected that supports markdown content, multi-language code tabs, search, and a responsive design
4. The selected framework integrates cleanly with the existing GitHub Actions CI/CD pipeline
5. Both decisions are documented with rationale and comparison against alternatives
6. The framework supports versioned documentation for future multi-version needs
7. Total cost at launch is $0/month (free tier sufficient for project scale)
8. Deployment can be triggered automatically on merge to `main`

## 2. Context and Prior Art

### 2.1 Current Documentation State

Vorpal's existing documentation consists of:

| Asset | Location | Content |
|-------|----------|---------|
| README | `README.md` | Install, quickstart, SDK examples, feature list |
| Architecture spec | `docs/spec/architecture.md` | System design, component architecture, data model, build flow |
| Operations spec | `docs/spec/operations.md` | CI/CD, service management, release process |
| Security spec | `docs/spec/security.md` | Auth, TLS, crypto, sandbox security |
| Performance spec | `docs/spec/performance.md` | Caching, compression, streaming, known bottlenecks |
| Code quality spec | `docs/spec/code-quality.md` | Coding standards |
| Testing spec | `docs/spec/testing.md` | Test strategy |
| Review strategy spec | `docs/spec/review-strategy.md` | Review workflow |
| CLI help | `vorpal --help` | Command reference |

This content forms the seed material for the documentation website. The `docs/spec/` files are comprehensive but are written as internal engineering specifications, not user-facing documentation.

### 2.2 Tech Stack Context

| Component | Technology |
|-----------|-----------|
| Core language | Rust |
| SDKs | Rust, Go, TypeScript (Bun runtime) |
| Build definition | Protocol Buffers / gRPC |
| CI/CD | GitHub Actions |
| Package registries | crates.io, npm |
| Container registry | Docker Hub |
| Source hosting | GitHub (`ALT-F4-LLC/vorpal`) |
| License | Apache 2.0 |

The project does not use Node.js in its core stack (the TypeScript SDK uses Bun), but the CI/CD pipeline already has `actions/setup-node` and `oven-sh/setup-bun` steps, so either Node or Bun-based tooling is viable for docs builds.

### 2.3 How Comparable Projects Solve This

| Project | Framework | Hosting | Notes |
|---------|-----------|---------|-------|
| Nix | Custom (Nix-built) | Netlify | Heavy custom tooling, not a good model for small teams |
| Bazel | Custom (Stardoc + Jekyll) | GitHub Pages via Firebase | Migrated multiple times; complexity reflects Google-scale needs |
| Buck2 | Docusaurus | GitHub Pages | Meta's build system; uses Docusaurus for React ecosystem alignment |
| Turborepo | Nextra | Vercel | Uses Vercel (company product); Nextra for Next.js ecosystem |
| Earthly | Docusaurus | Netlify | Good precedent -- similar project scale and audience |
| Gradle | Custom | Custom CDN | Enterprise scale, not applicable |
| Pants | Docusaurus | Netlify | Another build-system precedent using Docusaurus |
| Ruff | MkDocs (Material) | GitHub Pages | Python linter; MkDocs for Python ecosystem |
| Astral (uv) | Starlight/Astro | Netlify | Rust-based Python tooling; chose Starlight |

**Pattern:** Build systems targeting developer audiences overwhelmingly use either Docusaurus or Starlight/Astro, hosted on either GitHub Pages or Netlify/Vercel. Projects with React/JS ecosystem affinity lean Docusaurus; newer projects and those wanting lighter tooling lean Starlight.

## 3. Hosting Solution

### 3.1 Evaluation Criteria

| Criterion | Weight | Description |
|-----------|--------|-------------|
| Cost | High | Must be free at project scale (open-source, moderate traffic) |
| GitHub integration | High | Seamless deployment from GitHub Actions on merge to `main` |
| Operational overhead | High | Zero infrastructure management; team should focus on content, not infra |
| Preview deployments | High | Automatic per-PR preview URLs so documentation changes can be reviewed visually before merge |
| Custom domain support | Medium | Must support custom domain when ready (not required at launch) |
| Build performance | Low | Docs builds are small; build time is not a differentiator |
| CDN/Performance | Low | Static docs; all platforms provide adequate CDN |

**Note on preview deployments:** This criterion was originally weighted Medium but has been upgraded to High based on operator feedback. For a documentation site approaching v0.1.0 where contributor experience matters, the ability to visually review rendered documentation changes in a PR -- without requiring contributors to build locally -- is a significant DX advantage. GitHub Pages does not support this natively; platforms with built-in preview environments (Cloudflare Pages, Vercel, Netlify) have a meaningful advantage here.

### 3.2 Candidates

#### GitHub Pages

- **Cost:** Free for public repos (unlimited bandwidth for public sites)
- **GitHub integration:** Native -- deploy via `actions/deploy-pages` action, first-party support
- **Operational overhead:** Minimal; configuration is a single workflow file and repository setting
- **Custom domain:** Supported (CNAME + DNS, automatic HTTPS via Let's Encrypt)
- **Preview deployments:** Not built-in; requires workaround (deploy to separate branch/environment) or third-party action
- **Limitations:** Single branch/environment deployment model; no built-in branch previews; 1GB site size limit (more than sufficient); no server-side logic (static only)

#### Cloudflare Pages

- **Cost:** Free tier: unlimited sites, 500 builds/month, unlimited bandwidth
- **GitHub integration:** Native GitHub App integration; auto-deploys on push; also supports GitHub Actions deployment via `wrangler`
- **Operational overhead:** Low; requires Cloudflare account setup but no infrastructure management
- **Custom domain:** Supported with Cloudflare DNS (automatic HTTPS, edge caching)
- **Preview deployments:** Built-in; automatic preview URL per branch/PR
- **Limitations:** Requires Cloudflare account; 25 MiB max file size; build image may lag latest tool versions (mitigated by GitHub Actions-based builds)

#### Vercel

- **Cost:** Free tier (Hobby): unlimited deployments for personal/non-commercial use. Team plan required for organizations -- $20/user/month
- **GitHub integration:** Native GitHub App; auto-deploys; also supports GitHub Actions via `vercel` CLI
- **Operational overhead:** Low; managed platform
- **Custom domain:** Supported (automatic HTTPS)
- **Preview deployments:** Built-in; automatic preview per PR with commenting integration
- **Limitations:** Hobby plan restricts to personal/non-commercial use, which may conflict with an organization (`ALT-F4-LLC`) deploying on behalf of an open-source project. Team plan adds cost. Framework detection is opinionated (optimized for Next.js)

#### Netlify

- **Cost:** Free tier (Starter): 100GB bandwidth/month, 300 build minutes/month
- **GitHub integration:** Native GitHub App; auto-deploys; also supports GitHub Actions via `netlify-cli`
- **Operational overhead:** Low; managed platform
- **Custom domain:** Supported (automatic HTTPS via Let's Encrypt)
- **Preview deployments:** Built-in; automatic deploy preview per PR
- **Limitations:** Bandwidth cap (100GB) could matter at scale, though unlikely for docs. Build minutes cap (300/month) is generous for docs. Free tier limited to 1 concurrent build

### 3.3 Comparison Matrix

| Criterion | GitHub Pages | Cloudflare Pages | Vercel | Netlify |
|-----------|-------------|-----------------|--------|---------|
| Cost (free tier) | Free (unlimited) | Free (unlimited BW) | Free (Hobby only) | Free (100GB BW) |
| Org-compatible free tier | Yes | Yes | No (Hobby = personal) | Yes |
| GitHub Actions integration | Native (first-party) | Via wrangler/API | Via CLI | Via CLI |
| PR preview deployments | Manual setup | Built-in | Built-in | Built-in |
| Custom domain | Yes | Yes | Yes | Yes |
| Operational overhead | Lowest | Low | Low | Low |
| CDN quality | GitHub (Fastly) | Cloudflare (global) | Vercel (global) | Netlify (global) |
| Account required beyond GitHub | No | Yes (Cloudflare) | Yes (Vercel) | Yes (Netlify) |
| Already in CI ecosystem | Yes (GitHub native) | No | No | No |

### 3.4 Recommendation: Cloudflare Pages

**Cloudflare Pages** is the recommended hosting solution.

**Rationale:**

1. **Built-in preview deployments.** Every PR automatically gets a unique preview URL (e.g., `abc123.vorpal-docs.pages.dev`) with zero configuration. Reviewers can visually inspect rendered documentation changes directly from the PR without cloning the repo or building locally. This is a significant contributor experience advantage for a project approaching v0.1.0 that needs to encourage documentation contributions. GitHub Pages does not support this natively.

2. **Free for open-source with no meaningful caps.** Cloudflare Pages free tier provides unlimited bandwidth, unlimited sites, and 500 builds per month. For a documentation site that deploys on merge to `main` plus PR previews, 500 builds/month is more than sufficient. Unlike Vercel (Hobby plan restricts to personal/non-commercial use), Cloudflare Pages free tier has no organizational restrictions.

3. **GitHub integration via Wrangler.** Cloudflare Pages integrates with GitHub Actions via the `wrangler` CLI and the `cloudflare/wrangler-action` GitHub Action. The deployment workflow builds the site in GitHub Actions (where the project already has CI infrastructure) and pushes the output to Cloudflare Pages. This keeps the build step in the existing CI ecosystem while leveraging Cloudflare's hosting and preview infrastructure.

4. **Global CDN with edge caching.** Cloudflare's global edge network provides fast page loads worldwide. While all hosting platforms offer adequate CDN for static docs, Cloudflare's network is the largest and most performant.

5. **Custom domain with automatic HTTPS.** When the project is ready for a custom domain (e.g., `docs.vorpal.dev`), Cloudflare Pages provides automatic HTTPS provisioning. If the domain uses Cloudflare DNS, setup is a single click.

**Tradeoff acknowledged:** Cloudflare Pages requires a Cloudflare account, adding one external service dependency beyond GitHub. This is a modest operational cost -- a one-time account setup and a single `CLOUDFLARE_API_TOKEN` secret in GitHub Actions. The tradeoff is justified by the preview deployment capability that GitHub Pages cannot provide.

**Why not GitHub Pages:** GitHub Pages was the original recommendation due to its zero-external-dependency simplicity. However, the lack of native preview deployments is a significant gap for documentation contributor experience. Third-party workarounds (e.g., `rossjrw/pr-preview-action`) exist but are brittle, deploy to the same environment as production, and add maintenance burden. Cloudflare Pages provides preview environments as a first-class feature at zero additional cost.

**Why not Vercel or Netlify:** Vercel's free tier restricts to personal/non-commercial use, which is ambiguous for an organization (`ALT-F4-LLC`). Netlify is a viable alternative with similar preview capabilities, but its free tier has bandwidth (100GB/month) and build minute (300/month) caps that Cloudflare Pages does not. Between the two, Cloudflare Pages has the more generous free tier and better CDN performance.

## 4. Documentation Framework

### 4.1 Evaluation Criteria

| Criterion | Weight | Description |
|-----------|--------|-------------|
| Markdown support | High | Must render markdown content natively; existing docs are all markdown |
| Multi-language code blocks | High | Must support tabbed code examples (Rust, Go, TypeScript side by side) |
| Search | High | Built-in or easily integrated search for developer documentation |
| Ecosystem fit | High | Alignment with Vorpal's tech stack and contributor skill set |
| Documentation versioning | Medium | Ability to serve docs for multiple versions (important as project matures) |
| Customization | Medium | Theming, layout flexibility without fighting the framework |
| Build performance | Medium | Fast builds for quick iteration; not a blocker at current scale |
| Community and maintenance | Medium | Active maintenance, good documentation for the docs framework itself |
| Plugin ecosystem | Low | Extension points for future needs (API docs generation, etc.) |

### 4.2 Candidates

#### Docusaurus (v3)

- **Language:** JavaScript/TypeScript (React)
- **Runtime:** Node.js
- **Markdown support:** MDX (markdown + JSX components); frontmatter-based metadata
- **Code blocks:** Built-in tabbed code blocks via `Tabs` + `TabItem` MDX components
- **Search:** Algolia DocSearch (free for open-source) or local search plugins (`@easyops-cn/docusaurus-search-local`)
- **Versioning:** Built-in (`docusaurus docs:version`); snapshots docs directory per version
- **Customization:** React component swizzling; extensive theming API; CSS modules
- **Build performance:** Moderate; React-based SSG with webpack; can be slow for very large sites
- **Community:** Meta-backed; 60k+ GitHub stars; widely adopted (React, Jest, Babel, many build tools)
- **Output:** Static HTML/JS/CSS

#### Starlight (Astro)

- **Language:** TypeScript (Astro + optional React/Vue/Svelte components)
- **Runtime:** Node.js
- **Markdown support:** Native markdown + MDX support; frontmatter-based metadata
- **Code blocks:** Built-in `<Tabs>` component with `<TabItem>` for multi-language examples; Expressive Code integration for enhanced code blocks (line highlighting, titles, diffs)
- **Search:** Built-in Pagefind integration (client-side, zero-config, no external service)
- **Versioning:** Not built-in; community workaround via `starlight-versions` plugin (experimental)
- **Customization:** Astro component overrides; Tailwind CSS support; less mature than Docusaurus theming
- **Build performance:** Fast; Astro's partial hydration means less JS shipped to client; Vite-based builds
- **Community:** Astro-backed; growing rapidly (5k+ GitHub stars for Starlight); newer but actively maintained
- **Output:** Static HTML with minimal JS (islands architecture)

#### VitePress

- **Language:** TypeScript (Vue)
- **Runtime:** Node.js
- **Markdown support:** Markdown-it with Vue component injection; frontmatter-based
- **Code blocks:** Built-in code group syntax (`:::code-group`) for tabbed multi-language blocks
- **Search:** Built-in local search (MiniSearch) or Algolia
- **Versioning:** Not built-in; requires manual branch/directory management
- **Customization:** Vue component overrides; clean default theme; less swizzling surface area than Docusaurus
- **Build performance:** Very fast; Vite-based; minimal runtime JS
- **Community:** Vue team maintained; used by Vue, Vite, Rollup, Vitest; 14k+ stars
- **Output:** Static HTML with Vue hydration

#### Nextra (v4)

- **Language:** TypeScript (Next.js/React)
- **Runtime:** Node.js
- **Markdown support:** MDX; frontmatter-based
- **Code blocks:** Requires custom MDX components for tabs; less polished out-of-box than alternatives
- **Search:** Flexsearch-based local search (built-in)
- **Versioning:** Not built-in
- **Customization:** Next.js full flexibility; but also inherits Next.js complexity
- **Build performance:** Moderate; Next.js SSG build
- **Community:** Used by Turbopack, SWC; maintained by Vercel ecosystem contributors; ~4k stars
- **Output:** Static HTML/JS (Next.js export)

#### mdBook

- **Language:** Rust
- **Runtime:** Rust (native binary)
- **Markdown support:** CommonMark via pulldown-cmark; no MDX/component injection
- **Code blocks:** No built-in tabs; requires preprocessor plugins or raw HTML
- **Search:** Built-in (elasticlunr.js)
- **Versioning:** Not built-in
- **Customization:** Limited; Handlebars templates; minimal theming
- **Build performance:** Extremely fast (native Rust binary)
- **Community:** Rust project official tool; used by The Rust Programming Language book; 18k+ stars
- **Output:** Static HTML with minimal JS

### 4.3 Comparison Matrix

| Criterion | Docusaurus | Starlight | VitePress | Nextra | mdBook |
|-----------|-----------|-----------|-----------|--------|--------|
| Markdown | MDX | MD + MDX | MD + Vue | MDX | CommonMark |
| Multi-lang code tabs | Built-in (MDX) | Built-in (Tabs) | Built-in (code-group) | Manual MDX | Not built-in |
| Search | Algolia/local plugin | Pagefind (built-in) | MiniSearch (built-in) | Flexsearch (built-in) | elasticlunr (built-in) |
| Doc versioning | Built-in | Plugin (experimental) | Manual | Manual | Manual |
| Build speed | Moderate | Fast | Very fast | Moderate | Very fast |
| Client-side JS | Heavy (~300KB+) | Minimal (~50KB) | Moderate (~100KB) | Heavy (~300KB+) | Minimal |
| Theming/customization | Extensive | Good | Good | Extensive (Next.js) | Limited |
| Maintenance | Meta-backed | Astro-backed | Vue team | Community | Rust team |
| Maturity | High (v3, 5+ years) | Medium (v0.x, 2 years) | High (v1, 3+ years) | Medium (v4 recent) | High (5+ years) |
| Ecosystem alignment | JS/React | Agnostic (Astro) | JS/Vue | JS/React/Next | Rust |

### 4.4 Detailed Analysis

#### Docusaurus vs. Starlight (top two candidates)

These are the two strongest candidates for Vorpal's documentation. Here is a deeper comparison:

**Docusaurus strengths:**
- Built-in versioning is production-ready and battle-tested. As Vorpal matures past v0.1.0 and needs to maintain docs for multiple versions, this is a significant advantage.
- Largest ecosystem of plugins and community solutions. Anything you need has likely been solved.
- Proven at the exact use case: developer-facing documentation for build tools (Buck2, Earthly, Pants all use it).
- MDX enables embedding interactive components if needed (e.g., architecture diagrams, configuration generators).

**Docusaurus weaknesses:**
- Heavy client-side JS bundle (~300KB+). This is a developer experience concern, not a blocking issue, but it means slower page loads compared to Starlight.
- Webpack-based builds are slower than Vite-based alternatives.
- React dependency adds complexity for contributors who may not know React.

**Starlight strengths:**
- Significantly lighter output. Astro's islands architecture ships minimal JS to the client, resulting in faster page loads and better Lighthouse scores.
- Built-in Pagefind search requires no external service (Algolia) and no API keys. Zero-config.
- Expressive Code integration provides enhanced code blocks out of the box (line highlighting, file names, diff views) -- valuable for multi-language SDK documentation.
- Vite-based builds are faster than webpack.
- Framework-agnostic: does not force a React, Vue, or any other framework dependency on the project.

**Starlight weaknesses:**
- No built-in versioning. The `starlight-versions` plugin exists but is experimental. This is the most significant gap.
- Younger project (v0.x) with less battle-testing than Docusaurus.
- Smaller plugin ecosystem.

#### Why not the others

**VitePress:** Excellent framework, but Vue ecosystem alignment is weak for Vorpal (no Vue anywhere in the stack). The code-group syntax is clean, but VitePress is optimized for Vue ecosystem projects.

**Nextra:** Ties the docs to Next.js, adding significant build complexity for a static documentation site. The main advantage (Next.js flexibility) is unnecessary here.

**mdBook:** Strong Rust ecosystem alignment, but the lack of multi-language code tabs is a critical gap for a project with Rust, Go, and TypeScript SDKs. mdBook is ideal for single-language book-format documentation (like The Rust Programming Language) but not for multi-SDK reference documentation.

### 4.5 Recommendation: Starlight (Astro)

**Starlight** is the recommended documentation framework.

**Rationale:**

1. **Superior code block experience.** Vorpal's documentation will heavily feature side-by-side code examples in Rust, Go, and TypeScript (the README already demonstrates this pattern with `<details>` tags). Starlight's built-in `<Tabs>` component and Expressive Code integration provide the best out-of-box experience for this use case -- including syntax highlighting, line highlighting, file name labels, and diff views without custom components.

2. **Built-in search with zero external dependencies.** Pagefind search is included by default, runs entirely client-side, requires no Algolia account, no API keys, and no ongoing maintenance. For an open-source project trying to minimize operational overhead, this is a meaningful advantage over Docusaurus's search story (which either requires Algolia DocSearch application/approval or a third-party local search plugin).

3. **Performance and page weight.** Starlight ships significantly less JavaScript to the client than Docusaurus (~50KB vs. ~300KB+). For a documentation site that will be used globally by developers on varying connections, this matters. Lighthouse performance scores are consistently higher for Starlight sites.

4. **Framework agnosticism.** Vorpal is not a JavaScript/React project. It is a Rust build system with multi-language SDKs. Starlight does not impose React or any other frontend framework on the project, reducing the conceptual overhead for contributors who want to improve docs but do not know React.

5. **Modern build tooling.** Vite-based builds are faster than Docusaurus's webpack pipeline. For a documentation site that will be built on every merge to `main`, faster builds mean faster deployments.

6. **Active development trajectory.** Starlight is backed by the Astro team with a clear roadmap. The framework is rapidly gaining adoption among developer tools projects (the Astro project itself is widely used and well-funded).

**Tradeoff acknowledged:** Starlight lacks built-in documentation versioning. For v0.1.0 launch, this is acceptable -- there is only one version to document. This is a conscious, medium-risk deferral with a **hard decision checkpoint at v0.2.0 planning**. At that checkpoint, the team must evaluate:
- Whether the `starlight-versions` plugin has matured to production-ready status
- Whether a branch-based versioning strategy (build docs from tagged branches, deploy to versioned paths like `/v0.1/`, `/v0.2/`) is sufficient
- Whether to migrate to Docusaurus for its built-in versioning

The versioning decision must not be deferred past v0.2.0 planning. Delaying further would compound migration cost if a framework switch is needed.

**Migration path if needed:** If Starlight proves insufficient (versioning becomes critical and the plugin ecosystem does not mature, or the framework stalls in development), Docusaurus is the clear fallback. Both frameworks use markdown content with frontmatter, so content migration is straightforward. The main work would be converting Starlight-specific component syntax (`<Tabs>`, Expressive Code directives) to Docusaurus MDX equivalents.

## 5. Architecture

### 5.1 Package Manager

The `website/` directory will use **Bun** as its package manager.

**Rationale:** The project already uses Bun in its CI ecosystem (`oven-sh/setup-bun` in `vorpal.yaml`) and the TypeScript SDK uses Bun (`sdk/typescript/`). Using Bun for the documentation website aligns with the existing tooling rather than introducing a separate package manager. Bun reads `package.json` natively, so no configuration changes are needed if the team later decides to switch to npm.

### 5.2 Repository Structure

Documentation will live within the existing `vorpal` repository in a top-level `website/` directory:

```
vorpal/
  website/
    astro.config.mjs      # Starlight/Astro configuration
    package.json           # Dependencies (astro, @astrojs/starlight)
    bun.lock               # Bun lockfile
    tsconfig.json          # TypeScript config
    src/
      content/
        docs/              # Documentation pages (markdown)
          index.md         # Landing page
          getting-started/
            installation.md
            quickstart.md
          guides/
            rust.md
            go.md
            typescript.md
          concepts/
            architecture.md
            artifacts.md
            caching.md
            environments.md
          reference/
            cli.md
            configuration.md
            api.md
        config.ts          # Sidebar navigation, site metadata
      assets/              # Images, diagrams
    public/                # Static files (favicon, etc.)
```

**Why `website/` and not `docs/`:** The `docs/` directory already contains `spec/` and `tdd/` -- internal engineering documents that are distinct from user-facing documentation. Using `website/` avoids confusion between internal specs and the public documentation site.

### 5.3 Deployment Architecture

#### Production deployment (merge to `main`)

```
Developer merges PR to main
         |
         v
GitHub Actions workflow
  (website-deploy.yaml)
         |
         v
  bun install + astro build
         |
         v
  wrangler pages deploy website/dist/
  (cloudflare/wrangler-action)
         |
         v
  Live at: https://vorpal-docs.pages.dev
  (or custom domain when configured)
```

#### Preview deployment (pull request)

```
Developer opens/updates PR
         |
         v
GitHub Actions workflow
  (website-deploy.yaml)
         |
         v
  bun install + astro build
         |
         v
  wrangler pages deploy website/dist/
  --branch=$PR_BRANCH
         |
         v
  Preview at: https://<branch>.vorpal-docs.pages.dev
  (automatic unique URL per PR)
```

The deployment workflow:

1. Triggers on push to `main` (production deploy) and on pull requests touching `website/` (preview deploy)
2. Installs Bun and dependencies (`bun install` in `website/`)
3. Runs `astro build` to generate static output
4. Deploys via `cloudflare/wrangler-action` with `command: pages deploy website/dist/ --project-name=vorpal-docs`
5. For PRs, Wrangler automatically creates a preview deployment on a branch-specific URL

This is a single workflow file added to `.github/workflows/`. Two CI secrets are required: `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID`. A one-time Cloudflare account setup and project creation (`wrangler pages project create vorpal-docs`) is needed before the first deployment.

**Cloudflare account and credentials management:**

- **Account ownership:** The Cloudflare account must be owned by the `ALT-F4-LLC` organization (not a personal account). Document the account owner and at least one recovery contact in the team's shared operational documentation. If the account is tied to an individual, add a second team member as a Super Administrator to prevent single-point-of-failure on account access.
- **API token scoping:** The `CLOUDFLARE_API_TOKEN` must be scoped narrowly to **Cloudflare Pages edit permissions only** -- not account-wide API access. When creating the token in the Cloudflare dashboard, select "Edit Cloudflare Pages" under Account permissions and restrict it to the specific account. Do not grant Zone, DNS, or other unrelated permissions.
- **Token rotation:** Rotate the `CLOUDFLARE_API_TOKEN` at least annually or immediately if a team member with access departs. Document the rotation procedure in the project's operations runbook: (1) create a new token in Cloudflare dashboard with identical scoping, (2) update the `CLOUDFLARE_API_TOKEN` secret in GitHub repository settings, (3) verify a deployment succeeds, (4) revoke the old token.

**Base path configuration:** Cloudflare Pages serves content at the root of the deployment domain (e.g., `https://vorpal-docs.pages.dev/`), so no base path subpath is needed. The Astro configuration is:

```javascript
export default defineConfig({
  site: 'https://vorpal-docs.pages.dev',
  // base defaults to '/' -- no subpath needed
  // ...
});
```

If a custom domain is later configured (e.g., `docs.vorpal.dev`), only the `site` value changes -- a single-line config update with no content modifications required. All internal links in markdown content should use relative paths (e.g., `./quickstart` not `/quickstart`) for portability regardless of domain configuration.

### 5.4 Content Strategy

Documentation content will be organized into four tiers:

| Tier | Purpose | Examples |
|------|---------|---------|
| Getting Started | Zero-to-running onboarding | Installation, quickstart, first build |
| Guides | Task-oriented tutorials by SDK language | Building with Rust SDK, building with Go SDK, building with TypeScript SDK |
| Concepts | Explanation of how Vorpal works | Architecture, content-addressed caching, build flow, environments, executors |
| Reference | Lookup documentation | CLI commands, configuration options, API surface |

This follows the Diataxis documentation framework (tutorials, how-to guides, explanation, reference) which is the standard for developer documentation.

### 5.5 Integration with Existing Docs

The existing `docs/spec/` files are internal engineering specifications, not user-facing documentation. They will not be served directly on the website. Instead, they serve as source material:

- `docs/spec/architecture.md` informs the Concepts > Architecture page
- `docs/spec/operations.md` informs the Reference > CLI and Reference > Configuration pages
- `docs/spec/security.md` informs a future Guides > Security / Authentication page

The `README.md` quickstart content will be adapted (not duplicated) into the Getting Started section, with the README updated to link to the documentation website.

## 6. Risks and Open Questions

### 6.1 Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Starlight versioning gap becomes critical before plugin matures | Medium | Monitor `starlight-versions` plugin. **Hard decision checkpoint at v0.2.0 planning**: evaluate plugin maturity, assess whether branch-based versioning is sufficient, or trigger migration to Docusaurus. The versioning decision must be made before v0.2.0 ships -- do not defer past this point. Fallback options: (1) branch-based versioning with path-prefixed deploys, (2) migrate to Docusaurus |
| Documentation gets stale as project evolves rapidly | Medium | Integrate docs review into the PR review checklist. Documentation changes should be part of feature PRs when they affect user-facing behavior |
| Starlight project abandonment or stalled development | Low | Astro is well-funded and actively maintained. Migration to Docusaurus is feasible since content is portable markdown. Decision checkpoint: reassess if Starlight has no release in 6 months |
| Cloudflare Pages free tier changes or service degradation | Low | Cloudflare has a strong track record with free tier stability. The site is static HTML; migration to GitHub Pages or Netlify requires only changing the deploy action in CI (same build output). No vendor lock-in beyond the deployment step |
| External account dependency (Cloudflare) | Low | One-time setup cost. Only one secret (`CLOUDFLARE_API_TOKEN`) to manage. If the Cloudflare account becomes unavailable, fallback to GitHub Pages requires removing the wrangler step and adding `actions/deploy-pages` -- a single workflow file change |

### 6.2 Open Questions

1. **Custom domain:** Does the team want to use a custom domain (e.g., `docs.vorpal.dev` or `vorpal.dev`) at launch, or is `alt-f4-llc.github.io/vorpal/` sufficient for v0.1.0? This affects DNS configuration but not the hosting or framework decision.

2. **API reference generation:** Should the documentation site include auto-generated API reference from source code (e.g., `rustdoc` for Rust SDK, `godoc` for Go SDK, TypeDoc for TypeScript SDK)? This is orthogonal to the framework choice but affects the content strategy. If desired, generated docs can be linked externally or embedded as iframes.

3. **Blog:** Does the team want a blog/changelog section on the documentation site? Starlight supports a blog plugin (`starlight-blog`). This would be useful for release announcements and migration guides.

## 7. Testing Strategy

| Test | Method | Trigger |
|------|--------|---------|
| Build succeeds | `astro build` exits 0 | Every PR touching `website/` |
| Links are valid | `astro check` or a link-checking plugin | Every PR touching `website/` |
| Search index generates | Pagefind runs as part of build | Every build |
| Visual review | Local dev server (`astro dev`) | Developer workflow |

No end-to-end browser tests are needed at launch. The framework's built-in build validation catches structural issues (broken imports, invalid frontmatter, missing referenced files).

## 8. Implementation Phases

### Phase 1: Foundation (Size: S)

- Create Cloudflare Pages project (`vorpal-docs`) under the `ALT-F4-LLC` organization account
- Generate a narrowly scoped API token (Cloudflare Pages edit only) and document account ownership
- Add `CLOUDFLARE_API_TOKEN` and `CLOUDFLARE_ACCOUNT_ID` secrets to GitHub repository
- Initialize Starlight project in `website/` directory using npm
- Configure `astro.config.mjs` with site metadata and sidebar structure
- Create the Getting Started section (installation, quickstart) by adapting `README.md` content
- Add GitHub Actions workflow (`website-deploy.yaml`) for:
  - Production deployment to Cloudflare Pages on merge to `main`
  - Preview deployments on PRs touching `website/`
  - Build validation (`astro build`) on all PRs touching `website/`
- Verify production deployment and preview deployment work end-to-end

**Deliverable:** A live documentation site with basic getting-started content at `vorpal-docs.pages.dev`, with automatic PR preview environments and build validation in CI from day one.

### Phase 2: Core Content (Size: M)

- Create SDK-specific guides (Rust, Go, TypeScript) with tabbed code examples
- Create Concepts section (architecture, artifacts, caching, environments, executors)
- Create Reference section (CLI commands, configuration options)
- Add search configuration verification
- Update `README.md` to link to the documentation website

**Deliverable:** Comprehensive documentation covering all major Vorpal features and all three SDKs.

### Phase 3: Polish and Automation (Size: S)

- Add link validation to CI (in addition to the build check from Phase 1)
- Configure custom domain (if decided)
- Add contributing guide for documentation (how to add/edit pages, local preview workflow)

**Deliverable:** Enhanced CI validation and contributor onboarding documentation.

### Phase 4: Future Enhancements (Size: M, deferred)

- Versioned documentation (when `starlight-versions` matures or when v0.2.0 ships)
- Auto-generated API reference integration
- Blog/changelog section

**Dependency chain:** Phase 1 must complete before Phase 2. Phase 3 can run in parallel with Phase 2. Phase 4 is deferred and independent.
