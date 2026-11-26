// Expressive Code themes for the docs, built from the syntax tokens in the
// design system (docs-redesign stage 4, design-system.md, "Colour"). Each
// theme maps TextMate scopes onto five tokens: keyword, string, function
// (also types, attributes and keys), comment and number (also constants).
// Everything else is plain ink. Every token is at least 5.4:1 on the code
// background in its theme, so minSyntaxHighlightingColorContrast (4.5 in
// astro.config.mjs) leaves them as they are.

/** @param {{ name: string, type: 'light' | 'dark', bg: string, ink: string, kw: string, str: string, fn: string, com: string, num: string }} t */
function theme({ name, type, bg, ink, kw, str, fn, com, num }) {
	return {
		name,
		type,
		colors: {
			'editor.background': bg,
			'editor.foreground': ink,
		},
		tokenColors: [
			{ settings: { foreground: ink } },
			{
				scope: ['comment', 'punctuation.definition.comment', 'string.comment'],
				settings: { foreground: com, fontStyle: 'italic' },
			},
			{
				scope: [
					'keyword',
					'storage',
					'storage.type',
					'storage.modifier',
					'variable.language',
					'entity.name.tag',
					'markup.heading',
					'keyword.operator.new',
					'keyword.operator.expression',
					'keyword.operator.logical.python',
				],
				settings: { foreground: kw },
			},
			{
				// Operators and punctuation stay ink, apart from the keyword ones above.
				scope: ['keyword.operator', 'punctuation'],
				settings: { foreground: ink },
			},
			{
				scope: [
					'string',
					'string.quoted',
					'string.template',
					'string.regexp',
					'punctuation.definition.string',
					'markup.inline.raw',
				],
				settings: { foreground: str },
			},
			{
				scope: [
					'entity.name.function',
					'support.function',
					'meta.function-call.generic',
					'entity.name.type',
					'entity.name.class',
					'entity.name.namespace',
					'entity.other.inherited-class',
					'support.class',
					'support.type',
					'entity.other.attribute-name',
					'entity.name.command',
					'support.type.property-name',
					'meta.object-literal.key',
					'support.function.builtin.shell',
					'markup.underline.link',
				],
				settings: { foreground: fn },
			},
			{
				scope: [
					'constant.numeric',
					'constant.language',
					'constant.character',
					'constant.other',
					'support.constant',
					'variable.other.constant',
					'variable.other.enummember',
					'entity.name.lifetime.rust',
					'markup.bold',
				],
				settings: { foreground: num },
			},
			{ scope: 'markup.italic', settings: { fontStyle: 'italic' } },
			{ scope: 'markup.bold', settings: { fontStyle: 'bold' } },
		],
	};
}

export const vorpalLight = theme({
	name: 'vorpal-light',
	type: 'light',
	bg: '#fbfaf7',
	ink: '#1b1a17',
	kw: '#9a2a63',
	str: '#2c6a2a',
	fn: '#1d5a96',
	com: '#696760',
	num: '#8a5300',
});

export const vorpalDark = theme({
	name: 'vorpal-dark',
	type: 'dark',
	bg: '#181715',
	ink: '#ece9e1',
	kw: '#f39ac6',
	str: '#a4d68f',
	fn: '#8fc1f6',
	com: '#9a968b',
	num: '#e8b45e',
});
