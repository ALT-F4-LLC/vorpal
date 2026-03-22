---
project: "vorpal"
maturity: "implemented"
last_updated: "2026-03-22"
updated_by: "@team-lead"
scope: "SDK language carousel on the Vorpal documentation website landing page"
owner: "@ux-designer"
dependencies:
  - ../tdd/docs-website-hosting-and-framework.md
---

# SDK Language Carousel — Design Specification

## 1. Overview

### Surface Type

Web (Starlight/Astro static site, splash landing page). The carousel lives within an MDX page rendered by Starlight's `splash` template. All interactivity must use pure CSS transitions and minimal inline JavaScript — no external libraries.

### Users

| Attribute | Description |
|-----------|-------------|
| Role | Software engineers evaluating Vorpal as a build system |
| Skill level | Intermediate to senior; comfortable reading Go, Rust, or TypeScript code |
| Context | First or early visit to the Vorpal docs site; deciding whether to try Vorpal |
| Frequency | Low — most users see this page once or a few times during evaluation |

### Key Workflows (prioritized)

1. **Scan the value proposition.** User lands on the page and sees a code example rotating automatically, demonstrating that Vorpal works across multiple languages without scrolling.
2. **Select a preferred language.** User clicks a language tab to see the SDK they care about — install command and example code.
3. **Copy the install command or code.** User copies the install snippet or code example from the visible panel (existing Expressive Code copy button behavior).
4. **Navigate to a guide.** After seeing the example, user clicks through to the full SDK guide for their language.

### Success Criteria

| # | Criterion | How to verify |
|---|-----------|---------------|
| SC-1 | All three SDK sections (Go, Rust, TypeScript) are visible within a single viewport-height area, eliminating the need to scroll past three stacked sections. | Visual inspection at 1280px and 375px widths. |
| SC-2 | The carousel auto-rotates between languages on a fixed interval. | Observe the page without interaction for 2+ full rotation cycles. |
| SC-3 | Clicking a language tab immediately shows that language's content and resets the auto-rotation timer. | Click each tab; confirm instant switch and timer restart. |
| SC-4 | Transitions between panels use a smooth fade (no jarring cuts). | Visual inspection during auto-rotation and manual tab clicks. |
| SC-5 | The carousel is fully functional without JavaScript (content is accessible, just not animated). | Disable JS in browser; confirm all three SDK sections are visible as a static stack. |
| SC-6 | No external JS libraries are introduced. | Audit the built output; no new script tags or imports. |
| SC-7 | The existing Expressive Code copy button remains functional on the visible code block. | Click the copy button on each panel's code block; verify clipboard content. |
| SC-8 | The carousel is keyboard-accessible. | Tab to each language tab; press Enter/Space to activate; confirm focus management. |

### Success Metrics

| Metric | Measurement approach |
|--------|---------------------|
| Vertical scroll reduction | Measure the "Your build, your language" section height before and after. Target: at least 60% reduction. |
| Page load performance | Lighthouse performance score remains above 90. No new render-blocking resources. |

## 2. Information Architecture

### Data Model

The carousel presents three panels, each containing:

| Element | Source | Example (Go) |
|---------|--------|-------------|
| Language label | Tab text | "Go" |
| SDK card title | Card component | "Go SDK" |
| SDK card description | Card body | "For full-stack and platform engineers..." |
| Install command | Fenced code block inside Card | `go get github.com/ALT-F4-LLC/vorpal/sdk/go` |
| Example code | Fenced code block after Card | Full `main.go` contents |

### Navigation and Discoverability

- **Tab bar** sits above the carousel panels, providing a persistent, always-visible navigation mechanism.
- **Active tab** is visually distinct (see Section 5) so the user always knows which language is displayed.
- **Auto-rotation** provides passive discovery — users who do not click a tab will still see all three languages within 15 seconds.

### Information Hierarchy

1. Section heading ("Your build, your language.") and subtitle — unchanged.
2. **Tab bar** — top of the carousel widget, immediately below the subtitle.
3. **Active panel** — Card (title + install) and code block for the selected language.

## 3. Layout and Structure

### Desktop Layout (>= 640px)

```
+------------------------------------------------------------------+
|            Your build, your language.                              |
|  Choose the SDK that fits your project. All SDKs produce          |
|  identical artifacts.                                              |
|                                                                    |
|  +----------+----------+--------------+                           |
|  |   Go     |   Rust   |  TypeScript  |  <-- tab bar              |
|  +----------+----------+--------------+                           |
|  | ═══════════════════════════════════ | <-- active tab underline  |
|  |                                     |                           |
|  |  +-------------------------------+ |                           |
|  |  | Go SDK                    [i] | |                           |
|  |  | For full-stack and platform   | |                           |
|  |  | engineers who want builds ... | |                           |
|  |  |                               | |                           |
|  |  | ┌─────────────────────────┐   | |                           |
|  |  | │ $ go get github.com/...│   | |                           |
|  |  | └─────────────────────────┘   | |                           |
|  |  +-------------------------------+ |                           |
|  |                                     |                           |
|  |  ┌─────────────────────────────┐   |                           |
|  |  │ main.go                     │   |                           |
|  |  │ package main                │   |                           |
|  |  │                             │   |                           |
|  |  │ import (                    │   |                           |
|  |  │   ...                       │   |                           |
|  |  │ )                           │   |                           |
|  |  │                             │   |                           |
|  |  │ func main() {              │   |                           |
|  |  │   ...                       │   |                           |
|  |  │ }                           │   |                           |
|  |  └─────────────────────────────┘   |                           |
|  |                                     |                           |
|  | [■ ○ ○] <-- progress dots           |                           |
|  +-+-----------------------------------+                           |
+------------------------------------------------------------------+
```

### Mobile Layout (< 640px)

Same structure, but:
- Tab labels may truncate or use shorter names if needed (Go, Rust, TS). At current label lengths, no truncation is necessary.
- Code blocks use horizontal scroll (existing Expressive Code behavior).
- No swipe gestures — tabs and auto-rotation only.

### Tab Bar Dimensions

| Property | Value |
|----------|-------|
| Tab container | Full width of `.landing-section` (max-width: 52rem) |
| Tab distribution | Equal-width tabs, filling the container (3 tabs = 33.33% each) |
| Tab height | 48px (touch-friendly, meets 44px minimum WCAG target size) |
| Tab padding | 12px 16px |
| Active tab indicator | 3px solid underline, accent color, below the active tab |

## 4. Interaction Design

### 4.1 Auto-Rotation

| Property | Value | Rationale |
|----------|-------|-----------|
| Interval | 5 seconds | Long enough to scan the Card content; short enough to demonstrate rotation within a natural attention window. |
| Direction | Forward (Go -> Rust -> TypeScript -> Go...) | Matches left-to-right reading order and tab order. |
| Behavior on page load | Starts immediately with Go visible | Go is the first tab; auto-rotation begins after the first 5-second hold. |
| Behavior on tab click | Reset timer; show selected panel; resume auto-rotation after 5 seconds of inactivity on the selected panel | Prevents the rotation from interrupting a user who just clicked a tab. |
| Behavior when page is hidden | Pause rotation (use `visibilitychange` API) | Prevents wasted computation and avoids disorienting the user when they return to the tab. |

### 4.2 Tab Click Interaction

1. User clicks a language tab.
2. The active tab indicator slides (or transitions) to the clicked tab.
3. The current panel fades out (opacity 1 -> 0, 200ms ease).
4. The new panel fades in (opacity 0 -> 1, 200ms ease).
5. The auto-rotation timer resets (the new panel stays visible for a full 5 seconds before rotation resumes).

Total transition duration: 400ms (200ms out + 200ms in, sequential, not overlapping). This feels responsive without being jarring.

> **Implementation note:** The implemented transition uses a 200ms crossfade (old panel fades out, new panel fades in on `transitionend`) rather than a sequential 400ms. This is snappier and was accepted as an improvement.

### 4.3 Keyboard Interaction

| Key | Action |
|-----|--------|
| Tab | Move focus to the tab bar, then between tabs (standard tab focus order) |
| Enter / Space | Activate the focused tab (show its panel) |
| Arrow Left / Arrow Right | Move between tabs when a tab has focus (ARIA tabs pattern) |

ARIA roles:
- Tab container: `role="tablist"`
- Each tab: `role="tab"`, `aria-selected="true|false"`, `aria-controls="panel-{lang}"`
- Each panel: `role="tabpanel"`, `id="panel-{lang}"`, `aria-labelledby="tab-{lang}"`

### 4.4 Progress Dots

Three small dots below the carousel panel indicate which panel is active:
- Active dot: filled (accent color)
- Inactive dots: outlined/dim (gray-4)
- Dots are decorative indicators, not interactive (no click handler) — the tab bar is the primary navigation.
- Dots are `aria-hidden="true"` since the tab bar already communicates the active panel.

### 4.5 No-JS Fallback

When JavaScript is disabled:
- ~~The tab bar renders but tabs are non-functional (no click handlers).~~ **Implementation deviation:** The tab bar is hidden entirely without JS (via `display: none`) — non-functional buttons are confusing UX.
- All three SDK panels are visible, stacked vertically, exactly as they appear today.
- The progress dots are hidden.
- This ensures content accessibility with zero-JS degradation.

Implementation approach: ~~Use a `<noscript>` style block or~~ A CSS class (`sdk-carousel--active`) toggled by JS controls carousel behavior. Panels default to visible and stacked; JS adds the class that hides inactive panels.

## 5. Visual and Sensory Design

### Color Palette (Semantic Tokens)

| Element | Dark mode | Light mode | Token |
|---------|-----------|------------|-------|
| Active tab text | `--sl-color-white` (#ffffff) | `--sl-color-white` (#17181c) | Existing |
| Inactive tab text | `--sl-color-gray-3` (#888b96) | `--sl-color-gray-3` (#545861) | Existing |
| Active tab underline | `--sl-color-accent` (#6c63ff) | `--sl-color-accent` (#6c63ff) | Existing |
| Tab bar background | transparent | transparent | — |
| Tab bar bottom border | `--sl-color-gray-5` (#353841) | `--sl-color-gray-5` (#c0c2c7) | Existing |
| Active progress dot | `--sl-color-accent` (#6c63ff) | `--sl-color-accent` (#6c63ff) | Existing |
| Inactive progress dot | `--sl-color-gray-4` (#545861) | `--sl-color-gray-4` (#888b96) | Existing |

All colors use existing Starlight design tokens. No new custom properties are introduced.

### Typography

| Element | Style |
|---------|-------|
| Tab label | `font-size: var(--sl-text-sm)` (0.875rem), `font-weight: 600`, `text-transform: none` |
| All other elements | Unchanged from current design (Card title, Card body, code blocks use existing Starlight/Expressive Code styles) |

### Spacing

| Property | Value |
|----------|-------|
| Gap between subtitle and tab bar | 2rem (matches existing `margin-bottom` on `.landing-subtitle`) |
| Gap between tab bar and panel | 1.5rem |
| Gap between Card and code block within a panel | Existing spacing (unchanged) |
| Gap between code block and progress dots | 1rem |
| Progress dot size | 8px diameter |
| Progress dot gap | 8px between dots |

### Transitions

| Transition | Property | Duration | Easing |
|------------|----------|----------|--------|
| Panel fade out | opacity | 200ms | ease |
| Panel fade in | opacity | 200ms | ease |
| Tab underline slide | left, width | 250ms | ease-in-out |
| Progress dot fill | background-color | 200ms | ease |

All transitions use CSS `transition` property. No `@keyframes` animations are needed.

### Respects `prefers-reduced-motion`

When `prefers-reduced-motion: reduce` is active:
- All transitions are instant (duration: 0ms).
- Auto-rotation continues (it is not motion; it is content change). The fade is removed, so panels swap instantly.

## 6. Edge Cases and Error States

### Empty States

Not applicable — the three SDK panels are hardcoded in MDX. There is no dynamic content loading.

### Code Block Overflow

Long code lines (especially the Go import paths) already use horizontal scroll via Expressive Code. The carousel does not change this behavior. The carousel container must not clip the code block's horizontal scroll area.

### Rapid Tab Clicking

If the user clicks tabs rapidly (faster than the 400ms transition), the carousel should:
- Cancel any in-progress transition.
- Immediately show the most recently clicked tab's panel.
- No queuing of transitions.

Implementation: Track the "target panel" rather than queuing animations. Each click sets the target; the transition always goes to the current target.

### Panel Height Variance

The three SDK panels have different content heights (the Go example is longer than Rust and TypeScript). The carousel container should:
- Use the height of the **currently visible panel**, not the tallest panel.

> **Implementation note:** The fixed-height (tallest panel) approach was initially implemented but caused excessive whitespace below shorter panels (TypeScript). The final implementation uses natural panel sizing — each panel determines the container height when active. This means slight layout shifts between panels of different heights, but avoids the whitespace problem.

### Page Resize

If the viewport is resized while the carousel is visible:
- The carousel should remain functional. CSS handles responsive layout; no resize listener is needed.
- The tab bar remains full-width within the `.landing-section` container.

### Browser Back/Forward

The carousel does not interact with browser history. Tab state is ephemeral. Navigating away and back resets to the first tab (Go) with auto-rotation restarted.

## 7. Accessibility

### Keyboard Navigation

- Tab bar follows the [ARIA Tabs Pattern](https://www.w3.org/WAI/ARIA/apg/patterns/tabs/).
- Focus is visually indicated with a 2px outline using `--sl-color-accent` with 2px offset (matching Starlight's existing focus styles).
- Arrow keys move between tabs; Enter/Space activates. Home/End move to first/last tab.

### Screen Reader Semantics

- Tab bar is announced as "SDK language selector" (`aria-label="SDK language selector"` on the `tablist`).
- Each tab announces its language name and selected state.
- Panel content is linked to tabs via `aria-controls` / `aria-labelledby`.
- Auto-rotation does not use `aria-live` — the content change is driven by the tab's `aria-selected` state, which screen readers track when the user interacts with the tablist. Unsolicited auto-rotation announcements would be disruptive.

### Color Independence

- Active tab is indicated by underline position AND text weight (bold vs. regular) — not color alone.
- Progress dots use filled vs. outlined shape distinction in addition to color.

### Motion Sensitivity

- `prefers-reduced-motion: reduce` disables all CSS transitions (see Section 5).
- Auto-rotation persists (it changes content, not position/animation).

## 8. Internationalization

Not applicable for MVP. The tab labels ("Go", "Rust", "TypeScript") are programming language names and do not require translation. If the site is later internationalized, the Card descriptions and install instructions would need translation, but the carousel mechanism itself is language-agnostic.

## 9. Privacy and Data Minimization

No data is collected, stored, or transmitted by the carousel. All logic is client-side. No analytics instrumentation is specified for MVP.

## 10. Measurement

| Metric | How to measure |
|--------|----------------|
| Section height reduction | Compare "Your build, your language" section height before/after in browser dev tools. |
| Lighthouse performance | Run Lighthouse before/after. Target: no regression below 90 performance score. |
| Carousel JS weight | Measure inline script size in built output. Target: under 1KB minified. |

## 11. Handoff Notes

### Component Breakdown

The carousel is a single self-contained widget within `index.mdx`. It does not warrant a reusable Astro component since it is used exactly once on the splash page.

| Component / Element | Implementation Approach |
|---------------------|------------------------|
| **Tab bar** | HTML `<div>` with `role="tablist"`. Three `<button>` elements with `role="tab"`. Styled with CSS using existing Starlight tokens. |
| **Panel container** | A `<div>` wrapping three panel `<div>`s (one per language). Inactive panels are hidden via `opacity: 0; position: absolute; pointer-events: none;` (not `display: none`, to preserve Expressive Code initialization). |
| **Individual panel** | Each panel contains the existing `<Card>` component and fenced code block. The MDX content for each panel is identical to what exists today — just wrapped in a container `<div>`. |
| **Progress dots** | Three `<span>` elements styled as circles. `aria-hidden="true"`. |
| **Auto-rotation logic** | Script in `CarouselScript.astro` component (extracted from MDX because MDX parses `{}` as JSX). `setInterval` with `clearInterval` on tab click. `document.addEventListener('visibilitychange', ...)` to pause when page is hidden. |
| **CSS transitions** | All transitions defined in `custom.css` within a `.sdk-carousel` namespace. No new CSS file needed. |

### Technology Recommendations

- **Astro component for script**: The carousel JS lives in `website/src/components/CarouselScript.astro` with `<script is:inline>`. MDX parses curly braces as JSX expressions, so inline `<script>` in MDX doesn't work. The component is imported and rendered as `<CarouselScript />` in the MDX. Estimated JS: ~75 lines, well under 1KB minified.
- **CSS in `custom.css`**: Add carousel styles to the existing `website/src/styles/custom.css`. Namespace all rules under `.sdk-carousel` to avoid collisions.
- **No new dependencies**: Zero new npm packages. Zero new Astro integrations.

### MVP vs. Polish Priorities

**MVP (must-have):**
- Tab bar with three language tabs
- Click-to-switch with fade transition
- Auto-rotation with 5-second interval
- Timer reset on tab click
- No-JS fallback (stacked panels)
- ARIA roles and keyboard support
- `prefers-reduced-motion` support
- Progress dots

**Polish (nice-to-have, follow-up):**
- Tab underline slide animation (currently instant underline move)
- ~~Dynamic panel height animation (start with fixed height)~~ **Implemented:** natural panel sizing used instead of fixed height
- ~~`visibilitychange` pause (can defer — low impact)~~ **Implemented** in MVP

### Open Questions

1. **Tab order**: This spec assumes Go -> Rust -> TypeScript (matching the current page order). Should the order change? Consider ordering by ecosystem popularity or strategic priority.
2. **Auto-rotation interval**: 5 seconds is specified. The team should validate this feels right during implementation review. Adjusting is a single constant change.
3. **Link to SDK guide**: Should each panel include a "Learn more" link to the full SDK guide (e.g., `/guides/go/`)? This would add a clear next-step CTA per language. Not included in this spec since the original page does not have per-SDK links, but it would be a natural enhancement.

### Dependencies

- This spec depends on the existing Starlight/Astro setup documented in `docs/tdd/docs-website-hosting-and-framework.md`.
- No blocking dependencies on other work.
- @senior-engineer should implement and @sdet should verify SC-1 through SC-8 after implementation.
