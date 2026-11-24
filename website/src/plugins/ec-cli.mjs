// @ts-check
/**
 * Expressive Code plugin for CLI blocks and their expected output
 * (docs-redesign stage 4, design-system.md §6, decision 6).
 *
 * - ```` ```bash prompt ```` marks a CLI block: each command line gets a `$`
 *   prompt, drawn by CSS (`components.css`), so Copy, which copies the
 *   block's source, never includes it. Comment lines and continuation lines
 *   (after a trailing `\`) get no prompt.
 * - ```` ```text output ```` marks its expected output: titled "Expected
 *   output" unless the fence gives its own `title`, and placed right under
 *   the CLI block by CSS.
 * - In an output block, a runtime value written as `<digest>`, `<system>` or
 *   any `<lowercase-name>` is shown as a dashed placeholder, so the page
 *   never shows an invented digest.
 */
import { ExpressiveCodeAnnotation, addClassName, definePlugin } from '@astrojs/starlight/expressive-code';
import { h } from '@astrojs/starlight/expressive-code/hast';

class Placeholder extends ExpressiveCodeAnnotation {
	/** @param {import('@astrojs/starlight/expressive-code').AnnotationRenderOptions} opts */
	render({ nodesToTransform }) {
		return nodesToTransform.map((node) => h('span.vp-ph', [node]));
	}
}

const PLACEHOLDER = /<[a-z][a-z0-9-]*>/g;

/** @returns {import('@astrojs/starlight/expressive-code').ExpressiveCodePlugin} */
export default function cliBlocks() {
	return definePlugin({
		name: 'vorpal-cli-blocks',
		hooks: {
			preprocessMetadata({ codeBlock }) {
				if (codeBlock.metaOptions.getBoolean('output') && !codeBlock.props.title) {
					codeBlock.props.title = 'Expected output';
				}
			},
			annotateCode({ codeBlock }) {
				if (!codeBlock.metaOptions.getBoolean('output')) return;
				for (const line of codeBlock.getLines()) {
					for (const m of line.text.matchAll(PLACEHOLDER)) {
						line.addAnnotation(
							new Placeholder({
								inlineRange: { columnStart: m.index, columnEnd: m.index + m[0].length },
							}),
						);
					}
				}
			},
			postprocessRenderedLine({ codeBlock, line, renderData }) {
				if (!codeBlock.metaOptions.getBoolean('prompt')) return;
				const lines = codeBlock.getLines();
				const i = lines.indexOf(line);
				const text = line.text.trim();
				const continued = i > 0 && lines[i - 1].text.trimEnd().endsWith('\\');
				if (text && !text.startsWith('#') && !continued) addClassName(renderData.lineAst, 'vp-cmd');
			},
			postprocessRenderedBlock({ codeBlock, renderData }) {
				if (codeBlock.metaOptions.getBoolean('prompt')) addClassName(renderData.blockAst, 'vp-cli');
				if (codeBlock.metaOptions.getBoolean('output')) addClassName(renderData.blockAst, 'vp-output');
			},
		},
	});
}
