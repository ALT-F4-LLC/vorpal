// @ts-check
/**
 * Wrap headings with explicit ids, and the content under them, into the
 * design's bench steps, Check / Fix / Verify causes and reference entries
 * (docs-redesign stage 4, decision 14; critique-engineering B2).
 *
 * Authors write plain Markdown headings, so Starlight's "On this page",
 * its heading anchor links, Pagefind and the Markdown twins all see them.
 * Which headings are wrapped depends on the page's frontmatter `kind`:
 *
 * - tutorial, how-to, operator-how-to: `## Title {#step-N}` starts a bench
 *   step that runs to the next h2. Runs of steps become one
 *   `<ol class="vp-steps">`; inside a step, prose goes in the left column
 *   and code (code blocks, SDK groups) in the right, side by side when the
 *   step is at least 760px wide (a container query in components.css).
 * - troubleshooting: `### Title {#cause-N}` starts a cause card that runs to
 *   the next h2 or h3. Its `#### Check`, `#### Fix` and `#### Verify`
 *   headings become the rows of a definition list (each with a word, an
 *   icon and its own ground); text before the first row is the cause's
 *   "why" line.
 * - reference-cli, reference-config, reference-sdk, reference-errors:
 *   `### name {#flag-…}` (or `arg-`, `env-`, `key-`, `method-`, `err-`; h3 or h4) starts a
 *   reference entry that runs to the next heading of its level or higher. Put a `<Props>` line right under
 *   the heading in MDX.
 *
 * The heading itself is left alone: Starlight's anchor link (the 28px link
 * control) is added beside it after this plugin runs.
 *
 * Only top-level content is wrapped. Sätteri hast plugin: the `before` hook
 * sees the whole tree and replaces the root's children once.
 *
 * @type {import('satteri').HastPluginEntry}
 */
const sectionize = {
	name: 'vorpal-sectionize',
	before(tree, ctx) {
		const astro = /** @type {{ frontmatter?: { kind?: string } } | undefined} */ (ctx.data.astro);
		const kind = astro?.frontmatter?.kind ?? '';
		const rule = RULES.find((r) => r.kinds.includes(kind));
		if (!rule || !Array.isArray(tree.children)) return;
		const kids = /** @type {any[]} */ (tree.children);
		if (!kids.some((n) => isStart(n, rule))) return;

		/** @type {any[]} */
		const out = [];
		/** @type {any} */
		let steps = null;
		for (let i = 0; i < kids.length; ) {
			const node = kids[i];
			if (!isStart(node, rule)) {
				// Whitespace between two steps stays inside their list.
				if (steps && isBlank(node)) {
					i++;
					continue;
				}
				steps = null;
				out.push(node);
				i++;
				continue;
			}
			let j = i + 1;
			// A section runs to the next heading of its own level or higher.
			while (j < kids.length && level(kids[j]) > level(node)) j++;
			const body = kids.slice(i + 1, j);
			if (rule.wrap === 'step') {
				if (!steps) {
					steps = el('ol', { className: ['vp-steps'] }, []);
					out.push(steps);
				}
				steps.children.push(step(node, body));
			} else if (rule.wrap === 'cause') {
				out.push(cause(node, body));
			} else {
				out.push(el('div', { className: ['vp-refent'] }, [node, ...body]));
			}
			i = j;
		}
		ctx.setField(tree, 'children', out);
	},
};

const RULES = [
	{ kinds: ['tutorial', 'how-to', 'operator-how-to'], tag: ['h2'], id: /^step-\d+$/, wrap: 'step' },
	{ kinds: ['troubleshooting'], tag: ['h3'], id: /^cause-[\w-]+$/, wrap: 'cause' },
	{
		kinds: ['reference-cli', 'reference-config', 'reference-sdk', 'reference-errors'],
		tag: ['h3', 'h4'],
		id: /^(flag|arg|env|key|method|err)-[\w.-]+$/,
		wrap: 'entry',
	},
];

/** @param {any} node @param {typeof RULES[number]} rule */
const isStart = (node, rule) =>
	node?.type === 'element' && rule.tag.includes(node.tagName) && rule.id.test(String(node.properties?.id ?? ''));

/** A heading's level, or 7 for anything else. @param {any} node */
const level = (node) => (node?.type === 'element' && /^h[1-6]$/.test(node.tagName) ? Number(node.tagName[1]) : 7);

/** @param {any} node */
const isBlank = (node) => node?.type === 'text' && !node.value.trim();

/** @param {string} tagName @param {Record<string, unknown>} properties @param {any[]} children */
const el = (tagName, properties, children) => ({ type: 'element', tagName, properties, children });

/** @param {any} node */
const classes = (node) => {
	const c = node?.properties?.className ?? node?.properties?.class;
	return Array.isArray(c) ? c.map(String) : typeof c === 'string' ? c.split(/\s+/) : [];
};

/** Code goes in a step's right column: code blocks and SDK groups. @param {any} node */
const isCode = (node) =>
	(node?.type === 'element' &&
		(node.tagName === 'pre' || classes(node).some((c) => c === 'expressive-code' || c === 'vp-sdkg'))) ||
	(node?.type === 'mdxJsxFlowElement' && ['SdkGroup', 'Code', 'Diagram'].includes(node.name));

/** @param {any} heading @param {any[]} body */
function step(heading, body) {
	const text = body.filter((n) => !isCode(n));
	const code = body.filter(isCode);
	const cols = [el('div', { className: ['vp-step-t'] }, [heading, ...text])];
	if (code.length) cols.push(el('div', { className: ['vp-step-c'] }, code));
	return el('li', { className: ['vp-step'] }, [
		el('div', { className: code.length ? ['vp-step-g'] : ['vp-step-g', 'vp-step-g--text'] }, cols),
	]);
}

/** Icons for the cause rows (design-system.html, Check, Fix, Verify). */
const ICONS = {
	check: [
		el('circle', { cx: '8.5', cy: '8.5', r: '5.5', fill: 'none', stroke: 'currentColor', strokeWidth: '1.6' }, []),
		el('path', { d: 'M12.5 12.5l5 5', stroke: 'currentColor', strokeWidth: '1.6' }, []),
	],
	fix: [
		el(
			'path',
			{ d: 'M3 17l7.5-7.5M12 3.5a4 4 0 0 0 4.5 4.5l-2 2-4.5-4.5z', fill: 'none', stroke: 'currentColor', strokeWidth: '1.6' },
			[],
		),
	],
	verify: [el('path', { d: 'M3.5 10.5l4 4 9-9', fill: 'none', stroke: 'currentColor', strokeWidth: '2' }, [])],
};

/** @param {any} node */
const text = (node) =>
	node?.type === 'text'
		? node.value
		: Array.isArray(node?.children)
			? node.children.map(text).join('')
			: '';

/** @param {any} heading @param {any[]} body */
function cause(heading, body) {
	const why = [];
	/** @type {any[]} */
	const rows = [];
	/** @type {any} */
	let dd = null;
	for (const node of body) {
		if (node?.type === 'element' && node.tagName === 'h4') {
			const word = text(node).trim();
			const key = /** @type {keyof typeof ICONS | undefined} */ (
				Object.keys(ICONS).find((k) => word.toLowerCase().startsWith(k))
			);
			const icon = key
				? [el('svg', { viewBox: '0 0 20 20', ariaHidden: 'true', focusable: 'false' }, ICONS[key])]
				: [];
			rows.push(el('dt', { className: key ? [`vp-cfv-${key}`] : [] }, [...icon, { type: 'text', value: word }]));
			dd = el('dd', { className: key ? [`vp-cfv-${key}`] : [] }, []);
			rows.push(dd);
		} else if (dd) {
			dd.children.push(node);
		} else if (!isBlank(node)) {
			why.push(node);
		}
	}
	const children = [heading];
	if (why.length) children.push(el('div', { className: ['vp-cause-why'] }, why));
	if (rows.length) children.push(el('dl', { className: ['vp-cfv'] }, rows));
	return el('div', { className: ['vp-cause'] }, children);
}

export default sectionize;
