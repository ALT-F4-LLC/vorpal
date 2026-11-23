/**
 * Wrap every Markdown table in a named, focusable scroll region.
 *
 * Starlight makes a table its own horizontal scroller (`display: block;
 * overflow: auto`). At narrow widths, columns move out of view and a keyboard
 * user can reach them only if the scroller can take focus (WCAG 2.1.1). This
 * wraps each `<table>` in
 * `<div class="table-region" role="region" tabindex="0" aria-label="...">`,
 * the same pattern Expressive Code uses for code blocks. `custom.css` moves
 * the scrolling from the table to the wrapper.
 *
 * The accessible name is the table's `<caption>` if it has one. Otherwise it
 * is the nearest preceding heading, for example "Settings Keys table". When
 * that heading text repeats on the page (the CLI reference has an "Options"
 * heading per command), the parent heading is added: "Options table,
 * vorpal build". Names still equal after that are numbered. A table with no
 * heading before it is "Table N".
 *
 * @returns {(tree: any) => void}
 */
export default function rehypeTableRegion() {
	return (tree) => {
		/** Current heading text by level, index 2..6. @type {string[]} */
		const headings = [];
		/** @type {{ parent: any, index: number, table: any, own: string, context: string, parentHeading: string }[]} */
		const found = [];

		/** @param {any} node @returns {string} */
		const text = (node) =>
			node.type === 'text'
				? node.value
				: Array.isArray(node.children)
					? node.children.map(text).join('')
					: '';

		/** @param {any} parent */
		const walk = (parent) => {
			if (!Array.isArray(parent.children)) return;
			parent.children.forEach((/** @type {any} */ node, /** @type {number} */ index) => {
				if (node.type !== 'element') return walk(node);
				const level = /^h([2-6])$/.exec(node.tagName)?.[1];
				if (level) {
					headings[Number(level)] = text(node).trim();
					headings.length = Number(level) + 1;
					return;
				}
				if (node.tagName !== 'table') return walk(node);
				if (parent.properties?.className?.includes?.('table-region')) return;
				const chain = headings.filter(Boolean);
				const caption = node.children?.find(
					(/** @type {any} */ c) => c.type === 'element' && c.tagName === 'caption',
				);
				found.push({
					parent,
					index,
					table: node,
					own: caption ? text(caption).trim() : '',
					context: chain.at(-1) ?? '',
					parentHeading: chain.at(-2) ?? '',
				});
			});
		};
		walk(tree);

		/** @param {string[]} names */
		const counts = (names) =>
			names.reduce((m, n) => m.set(n, (m.get(n) ?? 0) + 1), new Map());
		const base = found.map((f) => (f.context ? `${f.context} table` : ''));
		const baseCount = counts(base);
		const named = found.map((f, i) => {
			if (f.own) return f.own;
			if (!base[i]) return '';
			const p = f.parentHeading;
			return baseCount.get(base[i]) > 1 && p ? `${base[i]}, ${p}` : base[i];
		});
		const total = counts(named);
		/** @type {Map<string, number>} */
		const seen = new Map();
		let unnamed = 0;
		found.forEach((f, i) => {
			let label = named[i];
			if (!label) label = `Table ${++unnamed}`;
			else if (total.get(label) > 1) {
				const n = (seen.get(label) ?? 0) + 1;
				seen.set(label, n);
				label = `${label} ${n}`;
			}
			f.parent.children[f.index] = {
				type: 'element',
				tagName: 'div',
				properties: {
					className: ['table-region'],
					role: 'region',
					tabIndex: 0,
					ariaLabel: label,
				},
				children: [f.table],
			};
		});
	};
}
