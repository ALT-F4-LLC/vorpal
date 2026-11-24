// @ts-check
import { readdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';
import { satteri } from '@astrojs/markdown-satteri';
import emDashes from './src/plugins/em-dashes.mjs';
import tableRegion from './src/plugins/table-region.mjs';

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
  // the page background brings it above 4.5:1.
  mermaid.initialize({
    startOnLoad: false,
    theme: next,
    themeVariables: next === 'dark' ? { edgeLabelBackground: '#17181c' } : {},
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
