/**
 * A small inline Markdown renderer for text kept in data files (the CLI
 * overlay, the errors collection, <Props> values): `code`, [text](href) and
 * **bold**; `blocks` also splits paragraphs on blank lines. Everything else
 * is escaped, so these files cannot inject HTML.
 */
export const esc = (s: string) =>
	s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** Inline Markdown: `code`, [text](href) and **bold**. Returns HTML. */
export function inline(md: string): string {
	let out = '';
	const re = /`([^`]+)`|\[((?:[^\]`]|`[^`]*`)+)\]\(([^)\s]+)\)|\*\*([^*]+)\*\*/g;
	let last = 0;
	for (const m of md.matchAll(re)) {
		out += esc(md.slice(last, m.index));
		if (m[1] !== undefined) out += `<code dir="auto">${esc(m[1])}</code>`;
		else if (m[2] !== undefined) out += `<a href="${esc(m[3])}">${inline(m[2])}</a>`;
		else out += `<b>${esc(m[4])}</b>`;
		last = (m.index ?? 0) + m[0].length;
	}
	return out + esc(md.slice(last));
}

/** Paragraphs of inline Markdown, split on blank lines. */
export const blocks = (md: string) =>
	md
		.split(/\n\s*\n/)
		.map((p) => p.trim())
		.filter(Boolean)
		.map((p) => `<p>${inline(p)}</p>`)
		.join('');

