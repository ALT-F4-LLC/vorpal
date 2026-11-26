# Vorpal Documentation Website

Built with [Starlight](https://starlight.astro.build). See [contributing docs](../docs/spec/code-quality.md) for development guidelines.

```bash
bun install
bun run dev     # local dev server at localhost:4321
bun run build   # production build to ./dist/
```

## Checks

On pull requests that touch `website/`, `script/website-checks/` or `.github/workflows/vorpal.yaml`, the `test-website-checks` and `test-website-lighthouse` jobs in `.github/workflows/vorpal.yaml` run size budgets, axe (WCAG 2.2 A/AA, light and dark), internal links and anchors (lychee), and Lighthouse CI. The tools live in `script/website-checks/`, outside `website/`, so they are not part of the `vorpal-website` artifact. To run them against a local build:

```bash
bun run build
cd ../script/website-checks
bun install
node size.mjs                       # budgets.json
node a11y.mjs                       # a11y-baseline.json; needs Playwright's Chromium (or CHROMIUM_PATH)
node_modules/.bin/lhci autorun --config=lighthouserc.json   # needs Chrome (or CHROME_PATH)
lychee --offline --include-fragments --index-files index.html \
  --root-dir "$PWD/../../website/dist" --exclude-path "$PWD/../../website/dist/pagefind" ../../website/dist
```

## Versions

The docs for main live at the site root. Each release line has a frozen snapshot at `/v/<x.y>/` (see `src/versions.mjs`): its pages in `src/content/docs/v/<x.y>/`, the data they render in `src/snapshots/<x.y>/`, its CLI data in `src/data/cli/releases/<x.y.z>.json`, and its sidebar in `src/snapshots/versions.json`. The build fails when the release in `package.json` has no snapshot.

When a release is cut, after `package.json` has the new version:

```bash
bun ../script/website-versions/snapshot.mjs
```

It writes the snapshot for that release (a patch release replaces its line's snapshot). Commit the result; don't edit a snapshot's files by hand, except to fix an error in the released docs. On main, a page that the latest release's snapshot doesn't have shows the Unreleased badge; CLI commands and flags are marked from the CLI data.
