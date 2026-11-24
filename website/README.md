# Vorpal Documentation Website

Built with [Starlight](https://starlight.astro.build). See [contributing docs](../docs/spec/code-quality.md) for development guidelines.

```bash
bun install
bun run dev     # local dev server at localhost:4321
bun run build   # production build to ./dist/
```

## Checks

Pull requests that touch `website/` run `.github/workflows/website.yaml`: size budgets, axe (WCAG 2.2 A/AA, light and dark), internal links and anchors (lychee), and Lighthouse CI. The tools live in `script/website-checks/`, outside `website/`, so they are not part of the `vorpal-website` artifact. To run them against a local build:

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
