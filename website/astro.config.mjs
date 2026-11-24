// @ts-check
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, fontProviders } from 'astro/config';
import starlight from '@astrojs/starlight';
import { satteri } from '@astrojs/markdown-satteri';
import emDashes from './src/plugins/em-dashes.mjs';
import tableRegion from './src/plugins/table-region.mjs';
import { vorpalDark, vorpalLight } from './src/code-themes.mjs';

/**
 * Prefix root-absolute internal links with the configured `base` at build
 * time. Authored links (markdown, MDX anchors, Starlight hero `link:`) are
 * written root-absolute, such as `/concepts/processes/#secrets`. Astro does
 * not rewrite these, so under a `base` they would point outside the site.
 * This hook rewrites `href` and `src` attributes inside HTML tags of the
 * built pages. Without a `base` it does nothing.
 *
 * Limits: it runs in `astro build` only, not in `astro dev`; it skips a value
 * that already starts with the base, so a base equal to a top-level section
 * name (such as `/reference`) would leave `/reference/...` links unprefixed;
 * it rewrites only double-quoted `href` and `src`, not `srcset` or others.
 *
 * @returns {import('astro').AstroIntegration}
 */
function prefixBaseLinks() {
	let base = '';
	return {
		name: 'prefix-base-links',
		hooks: {
			'astro:config:done': ({ config }) => {
				base = config.base.replace(/\/+$/, '');
			},
			'astro:build:done': async ({ dir }) => {
				if (!base) return;
				const isPrefixed = (/** @type {string} */ v) =>
					v === base || ['/', '#', '?'].some((c) => v.startsWith(base + c));
				const rewriteTag = (/** @type {string} */ tag) =>
					tag.replace(/(\s(?:href|src)=")(\/(?!\/)[^"]*)"/g, (m, attr, value) =>
						isPrefixed(value) ? m : `${attr}${base}${value}"`,
					);
				const root = fileURLToPath(dir);
				const entries = await readdir(root, { recursive: true });
				for (const entry of entries) {
					if (!entry.endsWith('.html')) continue;
					const file = join(root, entry);
					const html = await readFile(file, 'utf8');
					const out = html.replace(/<[a-zA-Z][^<>]*>/g, rewriteTag);
					if (out !== html) await writeFile(file, out);
				}
			},
		},
	};
}

// https://astro.build/config
export default defineConfig({
	site: 'https://docs.vorpal.build',
	// Self-hosted fonts: subset WOFF2 files committed in src/fonts (made by
	// script/website-fonts/subset.py; 39 KB together), so the build needs no
	// network and nothing outside bun.lock. `display: optional` plus Astro's
	// metric-matched fallback faces keep a late font from shifting layout.
	// Head.astro (an override) adds the @font-face rules and preloads.
	fonts: [
		{
			provider: fontProviders.local(),
			name: 'Instrument Sans',
			cssVariable: '--font-instrument-sans',
			display: 'optional',
			fallbacks: ['sans-serif'],
			options: {
				variants: [
					{ src: ['./src/fonts/instrument-sans-wght.woff2'], weight: '400 700', style: 'normal' },
				],
			},
		},
		{
			provider: fontProviders.local(),
			name: 'JetBrains Mono',
			cssVariable: '--font-jetbrains-mono',
			display: 'optional',
			fallbacks: ['monospace'],
			options: {
				variants: [
					{ src: ['./src/fonts/jetbrains-mono-wght.woff2'], weight: '400 700', style: 'normal' },
				],
			},
		},
	],
	markdown: {
		// Astro 7 renders Markdown and MDX with Sätteri. GFM (tables) is on by
		// default; MDX inherits this processor.
		processor: satteri({
			// Sätteri renders `--` as an en dash; em-dashes.mjs keeps it an em dash.
			features: { smartPunctuation: { dashes: false } },
			hastPlugins: [
				emDashes,
				// Make wide tables keyboard-reachable scroll regions (WCAG 2.1.1).
				tableRegion,
			],
		}),
	},
	integrations: [
		prefixBaseLinks(),
		starlight({
			title: 'Vorpal',
			head: [
				{
					tag: 'script',
					content: `document.addEventListener('DOMContentLoaded', () => {
						document.querySelectorAll('a[href*="github.com"]').forEach(a => {
							a.setAttribute('target', '_blank');
							a.setAttribute('rel', 'noopener noreferrer');
						});
					});`,
				},
				{
					// Render ```mermaid code blocks as diagrams in the browser.
					// - Exact version: a floating `mermaid@11` could change rendering
					//   under us. Bump deliberately after checking the diagrams.
					// - Theme follows Starlight's resolved theme
					//   (`<html data-theme="light|dark">`) and re-renders on toggle.
					// - Text equivalent: each diagram's source sets `accTitle` and
					//   `accDescr`, which Mermaid writes into the SVG as <title> and
					//   <desc>, referenced by aria-labelledby/aria-describedby.
					// - If Mermaid fails to load or a diagram fails to parse, the code
					//   block stays visible, so the source remains readable.
					tag: 'script',
					attrs: { type: 'module' },
					content: `import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11.17.2/dist/mermaid.esm.min.mjs';
const theme = () => (document.documentElement.dataset.theme === 'light' ? 'default' : 'dark');
const diagrams = Array.from(document.querySelectorAll('pre[data-language="mermaid"]'), (pre) => {
  const block = pre.closest('.expressive-code') ?? pre;
  const source = Array.from(pre.querySelectorAll('.ec-line'), (l) => l.textContent).join('\\n');
  const div = document.createElement('div');
  div.className = 'mermaid';
  div.hidden = true;
  block.after(div);
  return { block, div, source };
});
let rendered = '';
let count = 0;
async function render() {
  const next = theme();
  if (next === rendered) return;
  rendered = next;
  // The dark theme's edge-label background leaves its #ccc text at 4.43:1;
  // the page background (--vp-surface, dark) brings it above 4.5:1.
  mermaid.initialize({
    startOnLoad: false,
    theme: next,
    themeVariables: next === 'dark' ? { edgeLabelBackground: '#1c1b18' } : {},
  });
  for (const d of diagrams) {
    try {
      const { svg, bindFunctions } = await mermaid.render('vorpal-mermaid-' + count++, d.source);
      d.div.innerHTML = svg;
      bindFunctions?.(d.div);
      d.div.hidden = false;
      d.block.style.display = 'none';
    } catch (error) {
      console.error('Mermaid could not render a diagram; showing its source.', error);
    }
  }
}
let queue = Promise.resolve();
const schedule = () => (queue = queue.then(render));
if (diagrams.length) {
  schedule();
  new MutationObserver(schedule).observe(document.documentElement, {
    attributes: true,
    attributeFilter: ['data-theme'],
  });
}`,
				},
			],
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/ALT-F4-LLC/vorpal',
				},
			],
			customCss: ['./src/styles/tokens.css', './src/styles/shell.css', './src/styles/custom.css'],
			// Edit this page. Starlight appends the page's path from website/.
			editLink: {
				baseUrl: 'https://github.com/ALT-F4-LLC/vorpal/edit/main/website/',
			},
			// The page shell (docs-redesign step 3): sections, the narrowed
			// sidebar and the page header read this route data.
			routeMiddleware: './src/vorpal-route-data.ts',
			components: {
				Head: './src/components/overrides/Head.astro',
				Header: './src/components/overrides/Header.astro',
				Search: './src/components/overrides/Search.astro',
				ThemeSelect: './src/components/overrides/ThemeSelect.astro',
				MobileMenuToggle: './src/components/overrides/MobileMenuToggle.astro',
				Sidebar: './src/components/overrides/Sidebar.astro',
				PageTitle: './src/components/overrides/PageTitle.astro',
				MobileTableOfContents: './src/components/overrides/MobileTableOfContents.astro',
				Footer: './src/components/overrides/Footer.astro',
			},
			expressiveCode: {
				// Built from the design's syntax tokens (src/code-themes.mjs). The
				// first theme is the base; each applies under Starlight's
				// <html data-theme>.
				themes: [vorpalLight, vorpalDark],
				themeCssSelector: (theme) => `[data-theme='${theme.type}']`,
				// The tokens are AA-tested (at least 5.4:1 on the code background), so
				// a 4.5 floor leaves them untouched; Expressive Code's default (5.5)
				// would re-tint some of them.
				minSyntaxHighlightingColorContrast: 4.5,
				styleOverrides: {
					borderRadius: 'var(--vp-r-lg)',
					borderColor: 'var(--vp-line)',
					frames: {
						editorBackground: 'var(--vp-code)',
						terminalBackground: 'var(--vp-code)',
						editorTabBarBackground: 'var(--vp-chrome)',
						editorTabBarBorderBottomColor: 'var(--vp-line)',
						editorActiveTabBackground: 'var(--vp-code)',
						editorActiveTabForeground: 'var(--vp-ink)',
						editorActiveTabIndicatorTopColor: 'var(--vp-accent-fill)',
						editorActiveTabIndicatorBottomColor: 'transparent',
						terminalTitlebarBackground: 'var(--vp-chrome)',
						terminalTitlebarForeground: 'var(--vp-ink-2)',
						terminalTitlebarBorderBottomColor: 'var(--vp-line)',
						terminalTitlebarDotsForeground: 'var(--vp-line-strong)',
						inlineButtonForeground: 'var(--vp-ink)',
						inlineButtonBorder: 'var(--vp-line-strong)',
						tooltipSuccessBackground: 'var(--vp-ok)',
						tooltipSuccessForeground: 'var(--vp-surface)',
						frameBoxShadowCssValue: 'none',
					},
				},
			},
			sidebar: [
				{
					// The header's section names match these group labels (src/nav.ts).
					label: 'Get started',
					items: [
						{ label: 'Installation', slug: 'getting-started/installation' },
						{ label: 'Quickstart', slug: 'getting-started/quickstart' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Go', slug: 'guides/go' },
						{ label: 'Python', slug: 'guides/python' },
						{ label: 'Rust', slug: 'guides/rust' },
						{ label: 'TypeScript', slug: 'guides/typescript' },
					],
				},
				{
					label: 'Concepts',
					items: [
						{ label: 'Architecture', slug: 'concepts/architecture' },
						{ label: 'Artifacts', slug: 'concepts/artifacts' },
						{ label: 'Caching', slug: 'concepts/caching' },
						{ label: 'Environments', slug: 'concepts/environments' },
						{ label: 'Jobs', slug: 'concepts/jobs' },
						{ label: 'Processes', slug: 'concepts/processes' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Command-line (CLI)', slug: 'reference/cli' },
						{ label: 'Configuration', slug: 'reference/configuration' },
						{ label: 'API', slug: 'reference/api' },
						{ label: 'Contributing to Docs', slug: 'reference/contributing' },
					],
				},
			],
		}),
	],
});
