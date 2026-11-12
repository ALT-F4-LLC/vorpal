// @ts-check
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

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
	markdown: {
		// Astro 6 leaves `gfm` undefined by default, which @astrojs/mdx reads as
		// disabled, so `.mdx` pages lose GFM tables. Set it explicitly. The
		// build's `markdown.gfm` deprecation warning is expected; when upgrading
		// Astro, switch to an explicit `mdx({ gfm: true })` integration instead.
		gfm: true,
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
					tag: 'script',
					attrs: { type: 'module' },
					content: `import mermaid from 'https://cdn.jsdelivr.net/npm/mermaid@11/dist/mermaid.esm.min.mjs';
mermaid.initialize({ startOnLoad: false, theme: 'dark' });
document.querySelectorAll('pre[data-language="mermaid"]').forEach((pre) => {
  const ec = pre.closest('.expressive-code');
  const lines = pre.querySelectorAll('.ec-line');
  const text = Array.from(lines).map((l) => l.textContent).join('\\n');
  const div = document.createElement('div');
  div.classList.add('mermaid');
  div.textContent = text;
  if (ec) { ec.after(div); ec.style.display = 'none'; } else { pre.after(div); pre.style.display = 'none'; }
});
await mermaid.run({ querySelector: '.mermaid' });`,
				},
			],
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/ALT-F4-LLC/vorpal',
				},
			],
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Getting Started',
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
