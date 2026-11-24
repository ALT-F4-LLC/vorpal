/**
 * Render `--` in prose as an em dash (—).
 *
 * The docs are written with `--` for an em dash, which is what remark's
 * SmartyPants did before Astro 7. Sätteri's smart punctuation turns `--` into
 * an en dash (–) instead, so `astro.config.mjs` turns its dashes off and this
 * plugin restores the old output. Text inside `code`, `pre`, `kbd`, `samp`,
 * `script` and `style` is left alone, so CLI flags such as `--path` keep their
 * hyphens.
 *
 * @type {import('satteri').HastPluginEntry}
 */
const emDashes = {
	name: 'em-dashes',
	text(node, ctx) {
		if (!node.value.includes('--')) return;
		const skip = new Set(['code', 'pre', 'kbd', 'samp', 'script', 'style']);
		for (let p = ctx.parent(node); p; p = ctx.parent(p)) {
			if (p.type === 'element' && skip.has(p.tagName)) return;
		}
		ctx.replaceNode(node, { type: 'text', value: node.value.replaceAll('--', '—') });
	},
};

export default emDashes;
