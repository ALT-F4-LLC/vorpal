// @ts-check
/**
 * Where a page's Markdown copy lives (src/integrations/markdown-twins.mjs):
 * `/a/b/` -> `/a/b.md`, `/` -> `/index.md`; with `sdk`, `/a/b.python.md`.
 * @param {string} path the page's path, with its trailing slash
 * @param {string} [sdk]
 */
export const markdownPath = (path, sdk = '') =>
	(path.endsWith('/') ? (path === '/' ? '/index' : path.slice(0, -1)) : path) + (sdk ? `.${sdk}` : '') + '.md';
