/**
 * Links on a version snapshot's pages stay in that snapshot
 * (docs-redesign step 9; critique-engineering M3; src/versions.mjs).
 *
 * A snapshot's pages are copies of main's, so their Markdown, frontmatter
 * and components link root-absolute paths of main (`/concepts/caching/`).
 * This middleware rewrites them in the rendered HTML of every page under
 * `/v/<x.y>/`, in `astro dev` and `astro build` alike, so the Markdown
 * copies and search index see the rewritten links:
 *
 * - an `href` on `<a>` or `<link>` whose page exists in the snapshot moves
 *   under `/v/<x.y>/`, its `#fragment` and `?query` kept; so does a link to
 *   a page's Markdown copy (`/concepts/caching.md`, `.python.md`);
 * - everything else stays as it is: assets (`/_astro/`), search
 *   (`/pagefind/`), other snapshots, pages the snapshot does not have, and
 *   any tag marked `data-vp-version` (the version switcher and the banner,
 *   which link other versions on purpose).
 *
 * It never touches main's pages.
 */
import { defineMiddleware } from 'astro:middleware';
import { prefixOf, snapshotOf } from './versions.mjs';
import { versionPages } from './version-routes';

export const onRequest = defineMiddleware(async (context, next) => {
	const snapshot = snapshotOf(context.url.pathname);
	const response = await next();
	if (!snapshot) return response;
	const type = response.headers.get('content-type');
	if (type && !type.includes('text/html')) return response;
	const pages = (await versionPages()).snapshots.get(snapshot.slug) ?? new Set();
	const prefix = prefixOf(snapshot);

	/** The page a root-absolute path is for, as `/a/b/`, or undefined. */
	const pageOf = (path: string) => {
		const md = /^(.*?)(?:\.(?:python|typescript|go|rust))?\.md$/.exec(path);
		if (md) return md[1] === '/index' ? '/' : `${md[1]}/`;
		return path.endsWith('/') ? path : undefined;
	};
	const rewrite = (value: string) => {
		if (!value.startsWith('/') || value.startsWith('//')) return value;
		const path = value.replace(/[?#].*$/, '');
		if (/^\/(?:v|_astro|pagefind)\//.test(path)) return value;
		const page = pageOf(path);
		return page && pages.has(page) ? prefix + value : value;
	};

	const html = await response.text();
	const out = html.replace(/<(?:a|link)\s[^<>]*>/gi, (tag) =>
		tag.includes('data-vp-version') ? tag : tag.replace(/(\shref=")([^"]*)"/, (m, attr, v) => `${attr}${rewrite(v)}"`),
	);
	const headers = new Headers(response.headers);
	headers.delete('content-length');
	return new Response(out, { status: response.status, statusText: response.statusText, headers });
});
