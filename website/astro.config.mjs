// @ts-check
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig, fontProviders } from 'astro/config';
import starlight from '@astrojs/starlight';
import { satteri } from '@astrojs/markdown-satteri';
import emDashes from './src/plugins/em-dashes.mjs';
import tableRegion from './src/plugins/table-region.mjs';
import sectionize from './src/plugins/sectionize.mjs';
import { asideNotes, plannedDirective } from './src/plugins/callouts.mjs';

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
			// `attributes`: explicit heading ids (`## Title {#step-1}`), which
			// sectionize.mjs keys on.
			features: { smartPunctuation: { dashes: false }, attributes: true },
			// `:::planned[ENG-364]`; Starlight adds its asides after this.
			mdastPlugins: [plannedDirective],
			hastPlugins: [
				// Starlight's <aside> asides become role="note" callouts.
				asideNotes,
				// Bench steps, causes and reference entries, by frontmatter `kind`.
				sectionize,
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
			],
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/ALT-F4-LLC/vorpal',
				},
			],
			customCss: [
				'./src/styles/tokens.css',
				'./src/styles/shell.css',
				'./src/styles/components.css',
				'./src/styles/custom.css',
			],
			// Edit this page. Starlight appends the page's path from website/.
			editLink: {
				baseUrl: 'https://github.com/ALT-F4-LLC/vorpal/edit/main/website/',
			},
			// The page shell (docs-redesign step 3): sections, the narrowed
			// sidebar and the page header read this route data.
			routeMiddleware: './src/route-data.ts',
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
			// Code blocks: themes, styles and the CLI-block plugin live in
			// ec.config.mjs, so the <Code> component (SdkGroup) uses them too.
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
