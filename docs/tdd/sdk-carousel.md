---
project: "vorpal"
maturity: "implemented"
last_updated: "2026-03-22"
updated_by: "@team-lead"
scope: "Convert the three stacked SDK sections on the landing page into an auto-rotating carousel with tabbed navigation"
owner: "@staff-engineer"
dependencies:
  - docs-website-hosting-and-framework.md
  - ../ux/sdk-carousel.md
---

# SDK Language Carousel — Technical Design Document

## 1. Problem Statement

The Vorpal documentation landing page (`website/src/content/docs/index.mdx`) currently displays three SDK sections (Go, Rust, TypeScript) stacked vertically. Each section includes a Starlight `<Card>` component with an install command and a full code example. This forces visitors to scroll through approximately three viewport-heights of repetitive content to see the full SDK offering.

**Why now:** The documentation website is live on the `feature/docs-framework` branch and approaching merge to `main`. The landing page is the first impression for developers evaluating Vorpal. Compressing the SDK showcase into a single viewport area improves the scanning experience without removing any content.

**Constraints:**

- No external JavaScript libraries. The site currently has zero JS dependencies beyond Astro/Starlight built-ins.
- Pure CSS transitions with minimal inline JS. The Starlight `head` script pattern in `astro.config.mjs` (lines 14-20) establishes the precedent for inline client-side JS.
- Must preserve Expressive Code copy button functionality on all code blocks.
- Must work without JavaScript (graceful degradation to the current stacked layout).
- Must follow the ARIA tabs pattern for accessibility.
- Must conform to the UX design specification at `docs/ux/sdk-carousel.md`.

**Acceptance Criteria:**

| # | Criterion | Verification |
|---|-----------|-------------|
| AC-1 | All three SDK sections visible within a single viewport-height area at 1280px and 375px widths | Visual inspection |
| AC-2 | Carousel auto-rotates between Go, Rust, TypeScript on a 5-second interval | Observe without interaction for 2+ cycles |
| AC-3 | Clicking a language tab shows that language's content and resets the timer | Click each tab; confirm instant switch and timer restart |
| AC-4 | Transitions use a smooth opacity fade (200ms ease per direction) | Visual inspection during rotation and clicks |
| AC-5 | All content accessible with JavaScript disabled (stacked layout fallback) | Disable JS; confirm all three sections visible |
| AC-6 | No external JS libraries in built output | Audit `dist/` after build |
| AC-7 | Expressive Code copy button works on the visible panel's code block | Click copy button per panel; verify clipboard |
| AC-8 | Full keyboard navigation: Tab to tablist, Arrow Left/Right between tabs, Enter/Space to activate | Keyboard-only testing |
| AC-9 | `prefers-reduced-motion: reduce` disables fade transitions (instant swap) | Enable reduced-motion in OS/browser; verify |
| AC-10 | Lighthouse performance score remains above 90 | Run Lighthouse before and after |

## 2. Context and Prior Art

### Current Landing Page Structure

The "Your build, your language" section in `index.mdx` (lines 38-139) contains:

1. Section heading and subtitle
2. `<Card title="Go SDK">` with install command code block
3. Fenced Go code block (`main.go`)
4. `<Card title="Rust SDK">` with install command code block
5. Fenced Rust code block (`main.rs`)
6. `<Card title="TypeScript SDK">` with install command code block
7. Fenced TypeScript code block (`vorpal.ts`)

Each Card + code block pair is independent. The content is static MDX — no dynamic data fetching. The section is wrapped in a `<div class="landing-section">`.

### Existing CSS Patterns

`website/src/styles/custom.css` establishes:
- `.landing-section` — centered, `max-width: 52rem`, padded container
- `.landing-subtitle` — centered subtitle with `margin-bottom: 5rem`
- All custom properties use Starlight design tokens (`--sl-color-*`)
- Light/dark mode supported via `:root[data-theme='light']`

### Existing JS Patterns

`astro.config.mjs` (lines 14-20) injects a `DOMContentLoaded` script via the Starlight `head` config for adding `target="_blank"` to GitHub links. This establishes the pattern for small inline scripts. The carousel JS will follow the same approach (inline `<script>` in MDX) but placed directly in the MDX file rather than the global config, since it is page-specific.

### UX Spec

The UX design spec (`docs/ux/sdk-carousel.md`) provides detailed layout wireframes, interaction patterns, color tokens, spacing values, transition timings, accessibility requirements, and a no-JS fallback strategy. This TDD translates those design decisions into implementation architecture.

### How Similar Sites Handle This

- **Stripe docs** — Language selector tabs with code panels, opacity transitions, no external libs
- **Supabase** — SDK tabs with auto-rotation on landing page, pure CSS transitions
- **Tailwind CSS** — Tabbed code examples with smooth transitions in marketing pages

All use the same fundamental pattern: a tab bar controlling visibility of pre-rendered panels via opacity/position toggling.

## 3. Alternatives Considered

### Alternative A: Inline HTML in MDX (Recommended)

Write the carousel as raw HTML `<div>` elements directly in `index.mdx`, with a `<script>` tag for behavior and styles in `custom.css`.

**Strengths:**
- Zero new files or build configuration. Everything stays in the existing `index.mdx` and `custom.css`.
- The MDX content (Cards and fenced code blocks) remains as-is — just wrapped in container `<div>`s.
- Follows the precedent set by the existing landing page, which already uses raw HTML `<div>` wrappers around content sections.
- Easiest to understand and maintain — a single file contains the full carousel markup.
- No Astro component knowledge required from contributors.

**Weaknesses:**
- `index.mdx` grows longer (but the content is already there; only wrapper markup and the script are new).
- Not reusable — but the carousel is used exactly once, so reusability has no value.

### Alternative B: Astro Component

Create a `website/src/components/SdkCarousel.astro` component that encapsulates the tab bar, panel container, and carousel logic. Import it in `index.mdx`.

**Strengths:**
- Cleaner separation of concerns — carousel logic is isolated.
- If the carousel were used on multiple pages, this would enable reuse.

**Weaknesses:**
- Adds a new file that must be maintained.
- Astro components have constraints on how they interact with MDX content — the Card components and fenced code blocks would either need to be passed as slots (complex) or duplicated inside the component (defeats the purpose of MDX).
- Starlight's Expressive Code processes fenced code blocks at the MDX level. Moving code blocks inside an Astro component may break Expressive Code's copy button initialization, since Expressive Code hooks into the markdown pipeline, not component rendering.
- Over-engineering for a one-off widget.

### Alternative C: Starlight `<Tabs>` Component

Use Starlight's built-in `<Tabs>` and `<TabItem>` components, which already implement tabbed content panels.

**Strengths:**
- Zero custom JS needed — Starlight handles tab switching.
- Built-in ARIA support.
- Familiar pattern for Starlight users.

**Weaknesses:**
- No auto-rotation. Starlight's Tabs component is manual click-only. Adding auto-rotation would require monkey-patching the component's DOM, which is fragile across Starlight upgrades.
- No fade transition. Starlight Tabs uses `display: none` toggling, which means hidden panels are removed from the DOM — this breaks Expressive Code's copy button initialization on inactive panels.
- Cannot customize the transition mechanism (opacity vs. display) without forking the component.
- Does not support progress dots or timer reset behavior.

### Recommendation

**Alternative A (inline HTML in MDX)** is recommended. The carousel is a one-off landing page widget. Inline implementation keeps all carousel markup alongside the existing content it wraps, avoids Astro component complexity, and guarantees Expressive Code compatibility since the fenced code blocks remain in the MDX pipeline. The UX spec explicitly recommends this approach (Section 11, "Component Breakdown").

## 4. Architecture and System Design

### 4.1 Component Structure

The carousel is implemented as a self-contained section within `index.mdx`. No new Astro components, no new files beyond CSS additions to `custom.css`.

```
index.mdx
├── <div class="landing-section">
│   ├── ## Your build, your language.
│   ├── <p class="landing-subtitle">...</p>
│   │
│   ├── <div class="sdk-carousel" data-carousel>
│   │   ├── <div class="sdk-carousel-tabs" role="tablist" aria-label="SDK language selector">
│   │   │   ├── <button role="tab" id="tab-go"      aria-selected="true"  aria-controls="panel-go"      tabindex="0">Go</button>
│   │   │   ├── <button role="tab" id="tab-rust"    aria-selected="false" aria-controls="panel-rust"    tabindex="-1">Rust</button>
│   │   │   └── <button role="tab" id="tab-ts"      aria-selected="false" aria-controls="panel-ts"      tabindex="-1">TypeScript</button>
│   │   │
│   │   ├── <div class="sdk-carousel-panels">
│   │   │   ├── <div role="tabpanel" id="panel-go"   aria-labelledby="tab-go"   class="sdk-carousel-panel active">
│   │   │   │   ├── <Card title="Go SDK" icon="seti:go">...</Card>
│   │   │   │   └── ```go title="main.go" ... ```
│   │   │   │
│   │   │   ├── <div role="tabpanel" id="panel-rust" aria-labelledby="tab-rust" class="sdk-carousel-panel">
│   │   │   │   ├── <Card title="Rust SDK" icon="seti:rust">...</Card>
│   │   │   │   └── ```rust title="main.rs" ... ```
│   │   │   │
│   │   │   └── <div role="tabpanel" id="panel-ts"   aria-labelledby="tab-ts"   class="sdk-carousel-panel">
│   │   │       ├── <Card title="TypeScript SDK" icon="seti:typescript">...</Card>
│   │   │       └── ```typescript title="vorpal.ts" ... ```
│   │   │
│   │   └── <div class="sdk-carousel-dots" aria-hidden="true">
│   │       ├── <span class="sdk-carousel-dot active"></span>
│   │       ├── <span class="sdk-carousel-dot"></span>
│   │       └── <span class="sdk-carousel-dot"></span>
│   │
│   └── </div>
└── </div>
```

### 4.2 Panel Visibility Strategy

**Critical decision: Use `opacity` + `position: absolute`, NOT `display: none`.**

Starlight's Expressive Code integration initializes copy buttons and other interactive features during the initial page render. If panels are hidden with `display: none`, the browser does not lay out those elements, and Expressive Code's initialization may fail to measure or attach event handlers to the hidden code blocks.

The visibility approach:

| State | CSS Properties |
|-------|---------------|
| Active panel | `opacity: 1; position: relative; pointer-events: auto; z-index: 1;` |
| Inactive panel | `opacity: 0; position: absolute; pointer-events: none; z-index: 0; top: 0; left: 0; width: 100%;` |

The `.sdk-carousel-panels` container uses `position: relative` to contain the absolutely-positioned inactive panels. The active panel is `position: relative` so it participates in normal flow and determines the container's height.

### 4.3 Panel Height Strategy

**Recommendation: Fixed height (tallest panel) for MVP.**

The three panels have different content heights due to varying code block lengths. The UX spec (Section 6, "Panel Height Variance") recommends fixed height for MVP.

Implementation: The carousel JS measures all three panels on initialization, sets the container's `min-height` to the tallest panel's height, and recalculates on window resize (debounced). This avoids layout shifts during transitions.

```
On DOMContentLoaded:
  panels = querySelectorAll('.sdk-carousel-panel')
  maxHeight = Math.max(...panels.map(p => p.scrollHeight))
  container.style.minHeight = maxHeight + 'px'
```

**Why not pure CSS:** CSS cannot dynamically select `max()` across sibling element heights without all elements being in normal flow. Since inactive panels are `position: absolute`, they do not contribute to container height. JS measurement is required.

### 4.4 Transition Sequence

When switching from panel A to panel B (either by click or auto-rotation):

1. Panel A: add CSS class `sdk-carousel-panel--exiting` (triggers `opacity: 1 -> 0` over 200ms)
2. After 200ms (via `transitionend` event):
   - Panel A: remove `active` class, remove `--exiting` class
   - Panel B: add `active` class (triggers `opacity: 0 -> 1` over 200ms)
3. Update tab `aria-selected` states
4. Update progress dots

Total transition: 400ms (200ms fade-out + 200ms fade-in, sequential).

**Rapid click handling:** Each click cancels any in-progress transition by immediately removing all transitioning classes and setting the target panel as active. No transition queue. The JS tracks a single `targetIndex` variable — each click overwrites it.

### 4.5 Auto-Rotation Logic

```
State:
  currentIndex = 0
  timerId = null
  INTERVAL = 5000
  LANGUAGES = ['go', 'rust', 'ts']

startTimer():
  clearInterval(timerId)
  timerId = setInterval(() => {
    switchTo((currentIndex + 1) % 3)
  }, INTERVAL)

onTabClick(index):
  switchTo(index)
  startTimer()  // reset timer

onVisibilityChange():
  if (document.hidden) clearInterval(timerId)
  else startTimer()
```

The `visibilitychange` listener pauses rotation when the browser tab is hidden, preventing wasted computation and avoiding disorienting the user when they return.

### 4.6 No-JS Fallback

The carousel defaults to showing all panels visible (the current stacked layout). JavaScript adds the `sdk-carousel--active` class to the carousel container, which triggers the carousel CSS:

```css
/* Default: all panels visible, stacked (no-JS) */
.sdk-carousel-panel {
  opacity: 1;
  position: relative;
}

/* When JS activates the carousel */
.sdk-carousel--active .sdk-carousel-panel {
  opacity: 0;
  position: absolute;
  top: 0;
  left: 0;
  width: 100%;
  pointer-events: none;
  transition: opacity 200ms ease;
}

.sdk-carousel--active .sdk-carousel-panel.active {
  opacity: 1;
  position: relative;
  pointer-events: auto;
}
```

Without JS:
- The `--active` class is never added
- All panels remain `position: relative; opacity: 1`
- The tab bar renders but is non-functional (no click handlers)
- Progress dots are hidden via `.sdk-carousel:not(.sdk-carousel--active) .sdk-carousel-dots { display: none; }`

### 4.7 Keyboard Navigation

Following the [WAI-ARIA Tabs Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/):

| Key | Action |
|-----|--------|
| Arrow Right | Move focus to next tab (wraps from last to first) |
| Arrow Left | Move focus to previous tab (wraps from first to last) |
| Home | Move focus to first tab |
| End | Move focus to last tab |
| Enter / Space | Activate the focused tab |
| Tab | Move focus into/out of the tablist |

Focus management:
- Only the active tab has `tabindex="0"`. Inactive tabs have `tabindex="-1"`.
- Arrow keys update `tabindex` and call `.focus()` on the new tab.
- Enter/Space on a focused tab calls `switchTo()` and resets the auto-rotation timer.

## 5. CSS Architecture

All carousel styles are added to `website/src/styles/custom.css`, namespaced under `.sdk-carousel` to avoid collisions with existing styles.

### New CSS Rules

| Selector | Purpose |
|----------|---------|
| `.sdk-carousel` | Container; no visual effect until `--active` is added |
| `.sdk-carousel-tabs` | Tab bar: flexbox, full width, bottom border |
| `.sdk-carousel-tabs [role="tab"]` | Individual tab: equal width (33.33%), 48px height, accent underline when active |
| `.sdk-carousel-tabs [role="tab"][aria-selected="true"]` | Active tab: accent underline, bold text |
| `.sdk-carousel-tabs [role="tab"]:focus-visible` | Focus ring: 2px outline, accent color, 2px offset |
| `.sdk-carousel-panels` | Panel container: `position: relative` |
| `.sdk-carousel-panel` | Default: visible, stacked (no-JS) |
| `.sdk-carousel--active .sdk-carousel-panel` | Hidden: absolute, opacity 0 |
| `.sdk-carousel--active .sdk-carousel-panel.active` | Visible: relative, opacity 1 |
| `.sdk-carousel--active .sdk-carousel-panel--exiting` | Fading out: opacity 0 transition |
| `.sdk-carousel-dots` | Dot container: centered flexbox |
| `.sdk-carousel-dot` | Individual dot: 8px circle, gray-4 border |
| `.sdk-carousel-dot.active` | Active dot: filled with accent color |
| `@media (prefers-reduced-motion: reduce)` | All transition durations set to 0ms |

### Design Token Usage

Per the UX spec (Section 5), all colors use existing Starlight tokens:
- Active tab text: `--sl-color-white`
- Inactive tab text: `--sl-color-gray-3`
- Active underline: `--sl-color-accent`
- Tab bar border: `--sl-color-gray-5`
- Active dot: `--sl-color-accent`
- Inactive dot: `--sl-color-gray-4`

No new CSS custom properties are introduced. Both dark and light themes are supported automatically since the tokens already have theme-aware values in `custom.css` (lines 1-13 and 152-164).

## 6. Migration and Rollout

### Changes to Existing Files

| File | Change |
|------|--------|
| `website/src/content/docs/index.mdx` | Wrap the SDK section (lines 38-139) in carousel markup: tab bar `<div>`, three panel `<div>` wrappers around existing Card + code block pairs, progress dots `<div>`, inline `<script>` tag |
| `website/src/styles/custom.css` | Add `.sdk-carousel-*` rules. Remove `.landing-section .card + .expressive-code + .card { margin-top: 5rem; }` (line 70-72) since the spacing between stacked cards is handled by the carousel when active, and by normal flow when inactive |

### No Changes Required

| File | Why |
|------|-----|
| `astro.config.mjs` | No new plugins, integrations, or global scripts needed |
| `package.json` | No new dependencies |
| `custom.css` theme tokens | All needed tokens already exist |

### Rollout

This is a single-page change to a pre-merge documentation site. There is no phased rollout, feature flag, or rollback concern. The change ships as a single PR on the `feature/docs-framework` branch. If the carousel has issues, the fix is to remove the wrapper `<div>`s and script — the underlying MDX content is unchanged.

## 7. Risks and Open Questions

### Risks

| Risk | Severity | Mitigation |
|------|----------|------------|
| Expressive Code copy buttons fail on hidden panels | High | Use `opacity`/`position` visibility toggling instead of `display: none`. Verify copy functionality on all three panels during implementation review. |
| Panel height measurement incorrect on initial load (images, fonts not yet loaded) | Medium | Recalculate height on `load` event (not just `DOMContentLoaded`). Add a resize observer or debounced resize listener as a safety net. The code blocks are text-only, so font loading is the primary concern. |
| Starlight upgrade breaks inline `<script>` behavior in MDX | Low | Astro/Starlight explicitly supports `<script>` in MDX. This is a stable API. Pin Starlight version in `package.json` (currently `0.38.2`). |
| Auto-rotation is disorienting for users | Low | 5-second interval is generous. Any tab click resets the timer. `visibilitychange` pauses when tab is hidden. `prefers-reduced-motion` removes fade (but retains rotation). If feedback indicates rotation is unwanted, removing it is a one-line change (delete the `startTimer()` call). |

### Open Questions

1. **Tab order.** The UX spec assumes Go -> Rust -> TypeScript (matching current page order). Should this change? This is a content decision, not a technical one — the implementation supports any order.

2. **"Learn more" links.** The UX spec (Open Questions, item 3) asks whether each panel should include a link to the full SDK guide (`/guides/go/`, etc.). The current MDX content does not include these links. Adding them is trivial (an `<a>` tag inside each panel `<div>`) but is a content/UX decision.

3. **Auto-rotation interval tuning.** 5 seconds is specified in the UX spec. This is a single constant in the JS. Validated during implementation review — adjust if it feels too fast or slow in practice.

## 8. Testing Strategy

| Test Level | What | How |
|------------|------|-----|
| Visual | Carousel renders correctly at 1280px and 375px | Manual inspection during PR review; Cloudflare Pages preview deployment |
| Functional | Tab clicks switch panels, timer resets, copy buttons work | Manual testing per AC-3 and AC-7 |
| Accessibility | ARIA roles correct, keyboard nav works, focus management | Manual keyboard testing per AC-8; optionally run `axe` or Lighthouse accessibility audit |
| No-JS | All content visible when JS disabled | Disable JS in browser; verify per AC-5 |
| Performance | Lighthouse >= 90, no new render-blocking resources | Lighthouse audit per AC-10 |
| Reduced motion | Transitions disabled when `prefers-reduced-motion: reduce` | Toggle OS setting; verify per AC-9 |
| Cross-browser | Chrome, Firefox, Safari | Manual spot-check (CSS transitions and `visibilitychange` are supported in all modern browsers) |

No automated tests are needed for MVP. The carousel is a static-site UI widget with no server-side logic, no data fetching, and no state persistence. The testing strategy from the existing TDD (`docs-website-hosting-and-framework.md`, Section 7) — build succeeds, links are valid — remains sufficient for CI. Carousel-specific verification is manual during PR review, aided by the Cloudflare Pages preview deployment.

## 9. Observability and Operational Readiness

Not applicable. The carousel is a client-side UI widget on a static documentation site. There are no servers, no APIs, no metrics to monitor. If the carousel breaks, it degrades gracefully to the stacked layout (no-JS fallback). There is nothing to page on at 3am.

The only operational concern is Lighthouse performance regression (AC-10), which is checked during PR review, not via ongoing monitoring.

## 10. Implementation Phases

### Phase 1: CSS Foundation (Size: S) — ✅ Complete

Add all `.sdk-carousel-*` CSS rules to `custom.css`. This includes tab bar styling, panel visibility rules, transition definitions, progress dot styling, `prefers-reduced-motion` overrides, and the no-JS default state.

**Deliverable:** CSS is ready. No visual change yet (carousel markup not in MDX).

**Implementation notes:** Added `margin: 0` to tab buttons to override Starlight's `.sl-markdown-content` sibling spacing rule. Added `border-bottom: 3px solid transparent` to all tabs for consistent height.

### Phase 2: MDX Markup (Size: S) — ✅ Complete

Restructure the SDK section in `index.mdx`:
- Add the tab bar `<div>` with three `<button>` elements
- Wrap each Card + code block pair in a panel `<div>` with ARIA attributes
- Add progress dots markup
- Wrap everything in the `.sdk-carousel` container

**Deliverable:** Static carousel markup renders. Without JS, all panels are visible (stacked). The tab bar is visible but non-functional.

**Dependency:** Phase 1 (CSS must exist for correct rendering).

### Phase 3: JavaScript Behavior (Size: M) — ✅ Complete

Add the carousel script implementing:
- Carousel initialization (add `--active` class)
- Tab click handlers
- Auto-rotation (`setInterval`, timer reset on click)
- `visibilitychange` listener
- Keyboard navigation (arrow keys, Home/End, Enter/Space)
- `prefers-reduced-motion` detection (skip transitions)
- Rapid-click cancellation logic

**Deliverable:** Fully functional carousel matching all acceptance criteria.

**Dependency:** Phase 2 (markup must exist for JS to attach to).

**Implementation notes:** Script was extracted to `website/src/components/CarouselScript.astro` (with `is:inline`) because MDX parses curly braces in `<script>` tags as JSX expressions. Fixed-height (`min-height`) approach was removed in favor of natural panel sizing to avoid whitespace on shorter panels. Transition uses a 200ms crossfade rather than 400ms sequential fade-out/fade-in (snappier UX).

### Phase 4: Review and Polish (Size: S) — ✅ Complete

- Verify all acceptance criteria (AC-1 through AC-10)
- Test no-JS fallback
- Test keyboard navigation
- Run Lighthouse audit
- Adjust auto-rotation interval if needed
- PR review by @staff-engineer

**Dependency:** Phase 3.

**Parallelism:** Phases 1 and 2 were developed in parallel (different files). Phase 3 depended on both. Phase 4 was review/QA. Total scope: Small-Medium — CSS additions, MDX restructuring, and ~75 lines of inline JS in an Astro component.
