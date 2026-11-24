// @ts-check
/**
 * Markdown copies of every page, plus /llms.txt and /llms-full.txt
 * (docs-redesign step 7; final-design.md decision 3, critique-engineering.md
 * row 13a).
 *
 * After `astro build`, for every built page with Starlight content:
 * - `/<path>.md` holds the page as Markdown: its title, summary, body and
 *   Related links. Example groups keep all four SDKs, each under its own
 *   heading, as they read without JavaScript.
 * - `/<path>.<sdk>.md` holds the same page with only one SDK's examples
 *   (python, typescript, go, rust). Pages without examples get the same text,
 *   so the URL pattern always works.
 * - `/llms.txt` lists every page by section, linking the `.md` copies
 *   (https://llmstxt.org); `/llms-full.txt` is every page in one file.
 *
 * Why from the built HTML and not from the .mdx source: the CLI reference,
 * the error reference and the configuration keys are rendered from data by
 * Astro components, so their source holds no text. Converting the rendered
 * article covers every page with one code path. It reads only the article
 * (`.sl-markdown-content`) and markup this site owns (see `prepare`), and
 * takes code from Expressive Code's copy data, which is the original source
 * text, so code is never reconstructed from highlighted spans.
 *
 * The output depends only on dist/ and the sidebar config, so it is the same
 * on every machine. Links become absolute (`site` in astro.config.mjs).
 */
import { mkdir, readFile, readdir, writeFile } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { fromHtml } from 'hast-util-from-html';
import { select, selectAll } from 'hast-util-select';
import { toMdast } from 'hast-util-to-mdast';
import { toString } from 'hast-util-to-string';
import { gfmToMarkdown } from 'mdast-util-gfm';
import { toMarkdown } from 'mdast-util-to-markdown';
import { markdownPath } from '../markdown-path.mjs';

/** Site order (team decision 2). */
export const SDKS = /** @type {const} */ (['python', 'typescript', 'go', 'rust']);

/**
 * @typedef {{ label: string, slug?: string, link?: string, items?: SidebarItem[] }} SidebarItem
 * @typedef {{ path: string, title: string, description: string, markdown: string, sdkMarkdown: Record<string, string> }} Page
 * @typedef {import('hast').Element} Element
 * @typedef {import('hast').Root} Root
 * @typedef {import('hast').ElementContent} ElementContent
 */

/** @param {Element} node */
const classes = (node) => {
	const c = node.properties?.className;
	return Array.isArray(c) ? c.map(String) : [];
};
/** @param {Element} node @param {string} name */
const hasClass = (node, name) => classes(node).includes(name);

/** @param {string} tagName @param {ElementContent[]} children @returns {Element} */
const h = (tagName, children = []) => ({ type: 'element', tagName, properties: {}, children });
/** @param {string} value @returns {import('hast').Text} */
const text = (value) => ({ type: 'text', value });

/**
 * Walk the tree and let `fn` replace each element with zero or more nodes.
 * Children are visited after their parent is replaced.
 * @param {Root | Element} parent
 * @param {(node: Element) => ElementContent[] | undefined} fn
 */
function rewrite(parent, fn) {
	/** @type {any[]} */
	const out = [];
	for (const child of parent.children) {
		if (child.type !== 'element') {
			out.push(child);
			continue;
		}
		const replaced = fn(child);
		if (replaced === undefined) {
			rewrite(child, fn);
			out.push(child);
		} else {
			for (const r of replaced) {
				if (r.type === 'element') rewrite(r, fn);
				out.push(r);
			}
		}
	}
	parent.children = out;
}

/**
 * Turn the site's components into plain HTML that Markdown can express.
 * @param {Element} article
 * @param {string | undefined} sdk keep only this SDK's example panels
 */
function prepare(article, sdk) {
	rewrite(article, (node) => {
		const tag = node.tagName;
		// Controls, scripts and decoration.
		if (['script', 'style', 'link', 'button', 'template', 'form'].includes(tag)) return [];
		if (hasClass(node, 'sl-anchor-link') || hasClass(node, 'vp-sdkg-top') || hasClass(node, 'vp-jsonly'))
			return [];
		// An interactive scene (the Home hero) says nothing in Markdown.
		if (node.properties?.dataTwinSkip !== undefined) return [];
		if (tag === 'svg') {
			// A diagram keeps its name; its "In words" list follows it.
			const title = select('title', node);
			return title ? [h('p', [h('em', [text(`Diagram: ${toString(title).trim()}`)])])] : [];
		}
		// Expressive Code: the copy button holds the source text, with U+007F
		// for line breaks (the frame's `$` prompts are CSS, not text).
		if (hasClass(node, 'expressive-code')) {
			const pre = select('pre', node);
			const copy = select('button[data-code]', node);
			const title = select('figcaption .title', node);
			const lang = String(pre?.properties?.dataLanguage ?? '');
			const code = copy
				? String(copy.properties.dataCode).replace(/\u007f/g, '\n')
				: pre
					? toString(pre)
					: '';
			const titleText = title ? toString(title).trim() : '';
			/** @type {Element} */
			const codeEl = h('code', [text(code.replace(/\n+$/, ''))]);
			codeEl.properties.className = lang && lang !== 'plaintext' ? [`language-${lang}`] : [];
			/** @type {ElementContent[]} */
			const out = [];
			if (titleText) out.push(h('p', [h('em', [text(titleText)])]));
			out.push(h('pre', [codeEl]));
			return out;
		}
		// One SDK's copy: drop the other panels, and the panel heading.
		if (node.properties?.dataSdkPanel !== undefined && sdk) {
			if (node.properties.dataSdkPanel !== sdk) return [];
			return node.children.filter(
				(c) => !(c.type === 'element' && hasClass(c, 'vp-sdk-h')),
			);
		}
		// Callouts: a blockquote that starts with the label.
		if (hasClass(node, 'vp-callout')) {
			const label = select('.vp-callout-label', node);
			const body = select('.vp-callout-body', node);
			const issue = select('.vp-callout-issue', node);
			/** @type {ElementContent[]} */
			const lead = [h('strong', [text(`${toString(label ?? h('span')).trim()}:`)])];
			if (issue) lead.push(text(` (${toString(issue).trim()})`));
			const bodyChildren = body ? [...body.children] : [];
			const first = bodyChildren.find((c) => c.type === 'element');
			if (first && first.type === 'element' && first.tagName === 'p') {
				first.children.unshift(...lead, text(' '));
				return [h('blockquote', bodyChildren)];
			}
			return [h('blockquote', [h('p', lead), ...bodyChildren])];
		}
		// Definition lists (facts, properties, error details): "- **Term:** value".
		if (tag === 'dl') {
			/** @type {ElementContent[]} */
			const items = [];
			/** @type {Element | undefined} */
			let current;
			const visit = (/** @type {ElementContent[]} */ kids) => {
				for (const k of kids) {
					if (k.type !== 'element') continue;
					if (k.tagName === 'div') visit(k.children);
					else if (k.tagName === 'dt') {
						current = h('li', [h('strong', [text(`${toString(k).trim()}:`)])]);
						items.push(current);
					} else if (k.tagName === 'dd' && current) {
						const blocks = k.children.some(
							(c) => c.type === 'element' && ['p', 'ul', 'ol', 'pre', 'div', 'blockquote'].includes(c.tagName),
						);
						current.children.push(text(' '), ...k.children);
						if (blocks) current.children = [h('p', current.children.slice(0, 2)), ...current.children.slice(2)];
					}
				}
			};
			visit(node.children);
			return [h('ul', items)];
		}
		// The "Error" tag before an error message.
		if (hasClass(node, 'vp-etag')) return [text(`${toString(node).trim()}: `)];
		// Bench steps: the list holds headings, so unwrap it, and write the
		// number (drawn by CSS on the page) into each step's heading.
		if (hasClass(node, 'vp-steps')) {
			let n = 0;
			return node.children.flatMap((li) => {
				if (li.type !== 'element') return [];
				n += 1;
				const heading = select('h2, h3', li);
				if (heading) heading.children.unshift(text(`Step ${n}: `));
				return li.children;
			});
		}
		return undefined;
	});
}

/** @type {Parameters<typeof toMarkdown>[1]} */
const MD_OPTIONS = {
	bullet: '-',
	emphasis: '_',
	fence: '`',
	fences: true,
	rule: '-',
	listItemIndent: 'one',
	extensions: [gfmToMarkdown()],
};

/**
 * @param {Element} article
 * @param {string} pageUrl absolute URL of the HTML page, for links
 */
function toMd(article, pageUrl) {
	const mdast = toMdast(
		{ type: 'root', children: [article] },
		{
		},
	);
	// Absolute links, so a copy works outside the site.
	/** @param {any} node */
	const fix = (node) => {
		if ((node.type === 'link' || node.type === 'image') && node.url) {
			try {
				node.url = new URL(node.url, pageUrl).href;
			} catch {}
		}
		if (node.children) node.children.forEach(fix);
	};
	fix(mdast);
	return toMarkdown(mdast, MD_OPTIONS).trim();
}

/**
 * @param {string} html
 * @param {string} pageUrl
 * @param {string | undefined} sdk
 */
function convert(html, pageUrl, sdk) {
	const tree = fromHtml(html);
	const article = select('main .sl-markdown-content', tree);
	if (!article) return undefined;
	const h1 = select('main h1', tree);
	// A line break in the title (the Home hero's) reads as a space.
	if (h1) rewrite(h1, (node) => (node.tagName === 'br' ? [text(' ')] : undefined));
	const title = toString(h1 ?? h('span')).replace(/\s+/g, ' ').trim();
	const summaryEl = select('main .vp-summary', tree);
	const summary = summaryEl ? toString(summaryEl).trim() : '';
	const meta = select('meta[name="description"]', tree);
	const description = String(meta?.properties?.content ?? summary);
	const related = selectAll('.vp-related .vp-card a', tree).map((a) => ({
		title: toString(a).replace('→', '').trim(),
		href: new URL(String(a.properties.href), pageUrl).href,
	}));

	prepare(article, sdk);
	const parts = [`# ${title}`];
	if (summary) parts.push(`> ${summary}`);
	const body = toMd(article, pageUrl);
	if (body) parts.push(body);
	if (related.length) parts.push(['## Related', '', ...related.map((r) => `- [${r.title}](${r.href})`)].join('\n'));
	parts.push(`Source: ${pageUrl}`);
	return { title, description, markdown: parts.join('\n\n') + '\n' };
}


/**
 * @param {{ sidebar: SidebarItem[], title: string }} options
 * @returns {import('astro').AstroIntegration}
 */
export default function markdownTwins({ sidebar, title }) {
	/** @type {URL} */
	let site;
	let base = '/';
	return {
		name: 'vorpal-markdown-twins',
		hooks: {
			'astro:config:done': ({ config }) => {
				if (!config.site) throw new Error('markdown-twins needs `site` in astro.config.mjs');
				site = new URL(config.site);
				base = config.base.endsWith('/') ? config.base : `${config.base}/`;
			},
			'astro:build:done': async ({ dir, logger }) => {
				const root = fileURLToPath(dir);
				const files = (await readdir(root, { recursive: true }))
					.filter((f) => f.endsWith('index.html'))
					.map((f) => f.split('\\').join('/'))
					.sort();
				const siteRoot = new URL(base, site);
				/** @type {Map<string, Page>} */
				const pages = new Map();
				for (const file of files) {
					// dist/a/b/index.html -> /a/b/
					const path = '/' + file.replace(/index\.html$/, '');
					const pageUrl = new URL(path.slice(1), siteRoot).href;
					const html = await readFile(join(root, file), 'utf8');
					const all = convert(html, pageUrl, undefined);
					if (!all) continue;
					/** @type {Record<string, string>} */
					const sdkMarkdown = {};
					for (const sdk of SDKS) sdkMarkdown[sdk] = convert(html, pageUrl, sdk)?.markdown ?? all.markdown;
					pages.set(path, { path, ...all, sdkMarkdown });
				}

				const write = async (/** @type {string} */ path, /** @type {string} */ content) => {
					const out = join(root, path);
					await mkdir(dirname(out), { recursive: true });
					await writeFile(out, content);
				};
				for (const page of pages.values()) {
					await write(markdownPath(page.path), page.markdown);
					for (const sdk of SDKS) await write(markdownPath(page.path, sdk), page.sdkMarkdown[sdk]);
				}

				// llms.txt: pages by sidebar group, in sidebar order; then the rest.
				const mdUrl = (/** @type {string} */ path) => new URL(markdownPath(path).slice(1), siteRoot).href;
				const listed = new Set();
				const line = (/** @type {Page} */ p) => {
					listed.add(p.path);
					return `- [${p.title}](${mdUrl(p.path)})${p.description ? `: ${p.description}` : ''}`;
				};
				/** @param {SidebarItem[]} items @returns {Page[]} */
				const pagesIn = (items) =>
					items.flatMap((item) => {
						if (item.items) return pagesIn(item.items);
						const page = item.slug !== undefined ? pages.get(`/${item.slug}/`.replace('//', '/')) : undefined;
						return page ? [page] : [];
					});
				const home = pages.get('/');
				const sections = sidebar
					.map((group) => ({ label: group.label, pages: pagesIn(group.items ?? [group]) }))
					.filter((s) => s.pages.length);
				const llms = [`# ${title}`, ''];
				if (home) llms.push(`> ${home.description}`, '');
				llms.push(
					'Every page of these docs is also plain Markdown: add `.md` to its path (for example',
					`${mdUrl('/getting-started/quickstart/')}). Examples appear in all four SDKs;`,
					'for one SDK only, use `.python.md`, `.typescript.md`, `.go.md` or `.rust.md` instead.',
					`All pages in one file: ${new URL('llms-full.txt', siteRoot).href}`,
					'',
				);
				if (home) llms.push('## Start', '', line(home), '');
				for (const s of sections) llms.push(`## ${s.label}`, '', ...s.pages.map(line), '');
				const rest = [...pages.values()].filter((p) => !listed.has(p.path));
				if (rest.length) llms.push('## Optional', '', ...rest.map(line), '');
				await write('llms.txt', llms.join('\n'));

				const ordered = [...(home ? [home] : []), ...sections.flatMap((s) => s.pages), ...rest];
				const seen = new Set();
				const full = ordered
					.filter((p) => !seen.has(p.path) && seen.add(p.path))
					.map((p) => p.markdown.trim())
					.join('\n\n---\n\n');
				await write('llms-full.txt', `${full}\n`);
				logger.info(`Wrote ${pages.size} Markdown pages (x${SDKS.length + 1}), llms.txt and llms-full.txt.`);
			},
		},
	};
}
