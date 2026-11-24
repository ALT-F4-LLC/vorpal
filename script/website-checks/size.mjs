// Size budgets for the built docs site (website/dist).
//
// For every HTML page, adds up what a browser loads on first visit from the
// site itself: the HTML, the stylesheets it links (print ones too, since
// browsers fetch them), the scripts it loads plus their static imports, the
// fonts the stylesheets reference, and images and icons. Sizes are gzip, as
// served. Search (Pagefind) loads on demand and is not counted. Each page is
// checked against the budget of its type in budgets.json, and every
// third-party host a page loads from must be on the allow list.
//
// Usage: node size.mjs [--json out.json]   (WEBSITE_DIST overrides the dist path)
import { appendFileSync, existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, posix, relative, sep } from 'node:path';
import { gzipSync } from 'node:zlib';
import { DIST } from './serve.mjs';

const config = JSON.parse(readFileSync(new URL('./budgets.json', import.meta.url), 'utf8'));
const KB = 1024;
const gz = (/** @type {Buffer} */ buf) => gzipSync(buf, { level: config.gzipLevel }).length;
const allowed = new Map(config.thirdParty.allow.map((/** @type {any} */ a) => [a.host, a.reason]));

/** @type {Map<string, number>} */
const gzCache = new Map();
/** Gzip size of a file in dist by URL path, or undefined when missing. */
function fileSize(/** @type {string} */ urlPath) {
	if (!gzCache.has(urlPath)) {
		const file = join(DIST, ...urlPath.split('/'));
		gzCache.set(urlPath, existsSync(file) ? gz(readFileSync(file)) : -1);
	}
	const n = /** @type {number} */ (gzCache.get(urlPath));
	return n < 0 ? undefined : n;
}

const attr = (/** @type {string} */ tag, /** @type {string} */ name) =>
	new RegExp(`\\s${name}=(?:"([^"]*)"|'([^']*)'|([^\\s>]+))`, 'i').exec(tag)?.slice(1).find((v) => v !== undefined);

/** Resolve a reference against a page or file URL path. Returns null for data: and the like. */
function resolveRef(/** @type {string} */ ref, /** @type {string} */ from) {
	if (/^(data|blob|mailto|javascript):/i.test(ref)) return null;
	const url = new URL(ref, `https://site.invalid${from}`);
	if (url.host !== 'site.invalid') return { external: url.host };
	return { path: decodeURIComponent(url.pathname) };
}

/**
 * Imports of a JS file: static ones, and dynamic import() of a literal path.
 * Dynamic imports count because Starlight loads its search UI that way on
 * idle, on every page. Pagefind's own files are fetched at runtime by path,
 * not imported, so on-demand search stays out of the count.
 */
function jsImports(/** @type {string} */ source) {
	const out = [];
	const re = /(?:^|[;\s}),])(?:import|export)\s*(?:[\w*{}\s,$]+\s*from\s*)?["']([^"']+)["']|\bimport\(\s*["'`]([^"'`$]+)["'`]\s*\)/g;
	for (let m; (m = re.exec(source)); ) out.push(m[1] ?? m[2]);
	return out;
}

function pageType(/** @type {string} */ path) {
	return config.types.find((/** @type {any} */ t) => !t.match || new RegExp(t.match).test(path));
}

function measure(/** @type {string} */ htmlFile) {
	const rel = relative(DIST, htmlFile).split(sep).join('/');
	const path = '/' + rel.replace(/(^|\/)index\.html$/, '$1');
	const html = readFileSync(htmlFile, 'utf8');
	const row = { path, type: pageType(path).type, html: gz(Buffer.from(html)), css: 0, js: 0, fonts: 0, images: 0, requests: 1, thirdParty: new Set(), missing: [] };
	const seen = new Set();
	const add = (/** @type {string} */ kind, /** @type {string} */ ref, /** @type {string} */ from) => {
		const r = resolveRef(ref, from);
		if (!r) return null;
		if (r.external) return void row.thirdParty.add(r.external);
		if (seen.has(r.path)) return null;
		seen.add(r.path);
		const size = fileSize(r.path);
		if (size === undefined) return void row.missing.push(r.path);
		row[kind] += size;
		row.requests += 1;
		return r.path;
	};
	const addJs = (/** @type {string} */ ref, /** @type {string} */ from) => {
		const p = add('js', ref, from);
		if (p) for (const dep of jsImports(readFileSync(join(DIST, ...p.split('/')), 'utf8'))) addJs(dep, p);
	};
	for (const [tag] of html.matchAll(/<link\b[^>]*>/gi)) {
		const relAttr = (attr(tag, 'rel') ?? '').toLowerCase().split(/\s+/);
		const href = attr(tag, 'href');
		if (!href) continue;
		if (relAttr.includes('stylesheet')) {
			const p = add('css', href, path);
			if (p) {
				const css = readFileSync(join(DIST, ...p.split('/')), 'utf8');
				for (const [, , u] of css.matchAll(/url\((["']?)([^)"']+\.(?:woff2?|ttf|otf))\1\)/gi)) add('fonts', u, p);
			}
		} else if (relAttr.includes('modulepreload')) addJs(href, path);
		else if (relAttr.includes('preload') && attr(tag, 'as') === 'font') add('fonts', href, path);
		else if (relAttr.includes('icon')) add('images', href, path);
	}
	for (const [, tag, body] of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)) {
		const src = attr(` ${tag}`, 'src');
		if (src) addJs(src, path);
		else for (const dep of jsImports(body)) addJs(dep, path);
	}
	for (const [tag] of html.matchAll(/<(?:img|iframe|source|video)\b[^>]*>/gi)) {
		const src = attr(tag, 'src');
		if (src && !/\sloading=["']?lazy/i.test(tag)) add('images', src, path);
	}
	// Inline styles count in the HTML; their font URLs still load.
	for (const [, css] of html.matchAll(/<style\b[^>]*>([\s\S]*?)<\/style>/gi))
		for (const [, , u] of css.matchAll(/url\((["']?)([^)"']+\.(?:woff2?|ttf|otf))\1\)/gi)) add('fonts', u, path);
	row.total = row.html + row.css + row.js + row.fonts + row.images;
	return row;
}

const htmlFiles = readdirSync(DIST, { recursive: true, withFileTypes: true })
	.filter((d) => d.isFile() && d.name.endsWith('.html'))
	.map((d) => join(d.parentPath, d.name))
	.filter((f) => !relative(DIST, f).startsWith('pagefind'))
	.sort();
if (!htmlFiles.length) {
	console.error(`No HTML files in ${DIST}. Build the site first: cd website && bun run build`);
	process.exit(2);
}

const METRICS = ['html', 'css', 'js', 'fonts', 'total'];
const fmt = (/** @type {number} */ n) => (n / KB).toFixed(1);
const failures = [];
const rows = htmlFiles.map(measure);
const lines = [
	'| Page | Type | HTML | CSS | JS | Fonts | Images | **Total** | Requests | Third-party hosts |',
	'|---|---|---|---|---|---|---|---|---|---|',
];
for (const row of rows) {
	const budget = pageType(row.path);
	const cell = (/** @type {string} */ k) => {
		const over = row[k] > budget[k] * KB;
		if (over) failures.push(`${row.path} (${row.type}): ${k} ${fmt(row[k])} KB > budget ${budget[k]} KB`);
		return `${fmt(row[k])}${over ? ' ❌' : ''}`;
	};
	const req = row.requests > budget.requests;
	if (req) failures.push(`${row.path} (${row.type}): ${row.requests} requests > budget ${budget.requests}`);
	for (const host of row.thirdParty)
		if (!allowed.has(host)) failures.push(`${row.path}: loads from third-party host ${host}, which budgets.json does not allow`);
	for (const p of row.missing) failures.push(`${row.path}: references ${p}, which is not in dist/`);
	const hosts = [...row.thirdParty].map((h) => (allowed.has(h) ? `${h} (known)` : `${h} ❌`)).join(', ');
	lines.push(
		`| \`${row.path}\` | ${row.type} | ${cell('html')} | ${cell('css')} | ${cell('js')} | ${cell('fonts')} | ${fmt(row.images)} | **${cell('total')}** | ${row.requests}${req ? ' ❌' : ''} | ${hosts || 'none'} |`,
	);
}
const budgetLines = [
	'',
	'Budgets (KB gzip): ' +
		config.types
			.map((/** @type {any} */ t) => `${t.type}: ${METRICS.map((k) => `${k} ${t[k]}`).join(', ')}, requests ${t.requests}`)
			.join('; '),
	'',
	'Sizes are first-party bytes only. Third-party requests are not in dist/ and are not counted; their hosts are listed and must be allowed in budgets.json.',
];

const report = ['### Size budgets', '', ...lines, ...budgetLines].join('\n');
console.log(report);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, report + '\n\n');
const jsonOut = process.argv.indexOf('--json');
if (jsonOut > 0)
	writeFileSync(process.argv[jsonOut + 1], JSON.stringify(rows.map((r) => ({ ...r, thirdParty: [...r.thirdParty] })), null, 2));

if (failures.length) {
	console.error(`\n${failures.length} budget failure(s):\n- ${failures.join('\n- ')}`);
	process.exit(1);
}
console.log(`\nAll ${rows.length} pages are within budget.`);
