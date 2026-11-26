// @ts-check
/**
 * Callouts: Note, Warning and Planned (docs-redesign stage 4,
 * design-system.md §7; disposition m4).
 *
 * - `:::note`, `:::tip`, `:::caution` and `:::danger` are Starlight asides.
 *   Starlight renders `<aside aria-label>`, a complementary landmark, which
 *   axe flags when it sits inside another landmark. The hast plugin below
 *   rewrites each one into `<div role="note" aria-label>` with the design's
 *   word and icon. Caution's default word becomes "Warning".
 * - `:::planned[ENG-364]` is a Planned callout for work tracked in Linear.
 *   The mdast plugin turns it into the same note markup, before Starlight
 *   would drop an unknown directive. Planned never relies on colour: it has
 *   the word "Planned", a dashed border and icon, and the issue id, which is
 *   also in its accessible name ("Planned, ENG-364").
 */

/** 24px icons from design-system.html, as [tag, attributes] parts. */
const ICONS = {
	note: [
		['circle', { cx: '12', cy: '12', r: '9', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.8' }],
		['path', { d: 'M12 11v6M12 7.5v.5', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round' }],
	],
	warn: [
		['path', { d: 'M12 3.5l9.5 16.5h-19z', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.8', 'stroke-linejoin': 'round' }],
		['path', { d: 'M12 10v4.5M12 17.2v.3', stroke: 'currentColor', 'stroke-width': '2', 'stroke-linecap': 'round' }],
	],
	plan: [
		['circle', { cx: '12', cy: '12', r: '9', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.8', 'stroke-dasharray': '3 3' }],
		['path', { d: 'M12 7v5l3 2', fill: 'none', stroke: 'currentColor', 'stroke-width': '1.8', 'stroke-linecap': 'round' }],
	],
};
const VARIANT = { note: 'note', tip: 'note', caution: 'warn', danger: 'danger' };
const SVG = { class: 'vp-callout-icon', viewBox: '0 0 24 24', 'aria-hidden': 'true', focusable: 'false' };

/** The icon as hast. @param {keyof typeof ICONS} name */
const hastIcon = (name) => ({
	type: 'element',
	tagName: 'svg',
	properties: { className: ['vp-callout-icon'], viewBox: '0 0 24 24', ariaHidden: 'true', focusable: 'false' },
	children: ICONS[name].map(([tagName, attrs]) => ({
		type: 'element',
		tagName,
		properties: Object.fromEntries(
			Object.entries(attrs).map(([k, v]) => [k.replace(/-(\w)/g, (_, c) => c.toUpperCase()), v]),
		),
		children: [],
	})),
});

/** An mdast node rendered as an element (Starlight's own trick). @param {string} hName @param {Record<string, unknown>} hProperties @param {any[]} children */
const block = (hName, hProperties, children) => ({ type: 'paragraph', data: { hName, hProperties }, children });

/** The icon as mdast. @param {keyof typeof ICONS} name */
const mdastIcon = (name) => block('svg', SVG, ICONS[name].map(([tag, attrs]) => block(tag, attrs, [])));

/**
 * `:::planned[ENG-364]` → a Planned note. The label must be an issue id.
 *
 * @type {import('satteri').MdastPluginEntry}
 */
export const plannedDirective = {
	name: 'vorpal-planned',
	containerDirective(node, ctx) {
		if (node.name !== 'planned') return;
		const children = [...node.children];
		const first = /** @type {any} */ (children[0]);
		let issue = '';
		if (first?.type === 'paragraph' && first.data?.directiveLabel) {
			issue = ctx.textContent(first).trim();
			children.shift();
		}
		if (!/^[A-Z]+-\d+$/.test(issue)) {
			throw new Error(`:::planned needs a Linear issue id as its label, such as :::planned[ENG-364] (got "${issue}")`);
		}
		return /** @type {any} */ (
			block('div', { role: 'note', 'aria-label': `Planned, ${issue}`, class: 'vp-callout vp-callout--plan' }, [
				block('p', { class: 'vp-callout-label', 'aria-hidden': 'true' }, [
					mdastIcon('plan'),
					{ type: 'text', value: 'Planned ' },
					block('span', { class: 'vp-callout-issue' }, [{ type: 'text', value: issue }]),
				]),
				block('div', { class: 'vp-callout-body' }, children),
			])
		);
	},
};

/** @param {any} node */
const classes = (node) => {
	const c = node?.properties?.className ?? node?.properties?.class;
	return Array.isArray(c) ? c.map(String) : typeof c === 'string' ? c.split(/\s+/) : [];
};

/**
 * Starlight's `<aside>` asides → `<div role="note">` with the design's
 * markup.
 *
 * @type {import('satteri').HastPluginEntry}
 */
export const asideNotes = {
	name: 'vorpal-aside-notes',
	element: {
		filter: ['aside'],
		visit(node, ctx) {
			const cls = classes(node);
			if (!cls.includes('starlight-aside')) return;
			const type = cls.find((c) => c.startsWith('starlight-aside--'))?.slice('starlight-aside--'.length) ?? 'note';
			const variant = VARIANT[/** @type {keyof typeof VARIANT} */ (type)] ?? 'note';
			const kids = /** @type {any[]} */ (node.children ?? []);
			const title = kids.find((k) => classes(k).includes('starlight-aside__title'));
			const content = kids.find((k) => classes(k).includes('starlight-aside__content'));
			// The title holds Starlight's icon and then the words.
			const words = title ? (title.children ?? []).filter((/** @type {any} */ k) => !(k.type === 'element' && k.tagName === 'svg') && k.type !== 'raw' && k.type !== 'mdxJsxTextElement') : [];
			let label = String(node.properties?.ariaLabel ?? node.properties?.['aria-label'] ?? 'Note');
			let wordNodes = words;
			if (type === 'caution' && label === 'Caution') {
				label = 'Warning';
				wordNodes = [{ type: 'text', value: 'Warning' }];
			}
			const icon = hastIcon(variant === 'danger' ? 'warn' : /** @type {'note' | 'warn'} */ (variant));
			ctx.replaceNode(node, /** @type {any} */ ({
				type: 'element',
				tagName: 'div',
				properties: { role: 'note', ariaLabel: label, className: ['vp-callout', `vp-callout--${variant}`] },
				children: [
					{
						type: 'element',
						tagName: 'p',
						properties: { className: ['vp-callout-label'], ariaHidden: 'true' },
						children: [icon, ...wordNodes],
					},
					{
						type: 'element',
						tagName: 'div',
						properties: { className: ['vp-callout-body'] },
						children: content?.children ?? [],
					},
				],
			}));
		},
	},
};
