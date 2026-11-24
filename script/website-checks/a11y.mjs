// axe accessibility check over a representative set of built pages, in light
// and dark, at phone and desktop widths. Fails on WCAG 2.0-2.2 A/AA
// violations that are not in a11y-baseline.json.
//
// The baseline holds known violations as counts of affected nodes per page,
// color scheme, width and rule. The check fails when a new rule appears or a
// count grows, and reports counts that shrank so the baseline can be lowered.
//
// Usage:
//   node a11y.mjs                      check against the baseline
//   node a11y.mjs --update-baseline    rewrite a11y-baseline.json from this run
// Env: WEBSITE_DIST (default ../../website/dist), CHROMIUM_PATH (optional
// browser binary, for a preinstalled Chromium).
//
// Third-party requests are blocked, so the result does not depend on a CDN.
import { AxeBuilder } from '@axe-core/playwright';
import { appendFileSync, readFileSync, writeFileSync } from 'node:fs';
import { chromium } from 'playwright';
import { serve } from './serve.mjs';

export const PAGES = [
	'/',
	'/getting-started/quickstart/',
	'/guides/development-environment/',
	'/guides/typescript/',
	'/self-hosting/tls/',
	'/sdks/python/',
	'/concepts/how-vorpal-compares/',
	'/concepts/architecture/',
	'/concepts/jobs/',
	'/concepts/artifacts/',
	'/reference/cli/',
	'/reference/cli/build/',
	'/reference/configuration/',
	'/troubleshooting/errors/',
	'/404.html',
];
const SCHEMES = /** @type {const} */ (['light', 'dark']);
const WIDTHS = [390, 1280];
const TAGS = ['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22a', 'wcag22aa'];
const BASELINE = new URL('./a11y-baseline.json', import.meta.url);

const update = process.argv.includes('--update-baseline');
/** @type {{ known: Record<string, number> }} */
const baseline = JSON.parse(readFileSync(BASELINE, 'utf8'));

const { origin, close } = await serve();
const browser = await chromium.launch(
	process.env.CHROMIUM_PATH ? { executablePath: process.env.CHROMIUM_PATH } : {},
);
/** @type {Record<string, number>} */
const found = {};
/** @type {Record<string, { help: string, url: string, targets: string[] }>} */
const details = {};
/** Scan every page in one color scheme and width. */
async function scan(/** @type {'light' | 'dark'} */ scheme, /** @type {number} */ width) {
	const context = await browser.newContext({
		colorScheme: scheme,
		viewport: { width, height: 900 },
		reducedMotion: 'reduce',
	});
	await context.route(
		(url) => url.origin !== origin,
		(route) => route.abort(),
	);
	const page = await context.newPage();
	for (const path of PAGES) {
		await page.goto(origin + path, { waitUntil: 'networkidle' });
		// Let page scripts settle: Expressive Code makes overflowing code blocks
		// focusable from a ResizeObserver, after load.
		await page.evaluate(
			() => new Promise((done) => requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done, 250)))),
		);
		const theme = await page.evaluate(() => document.documentElement.dataset.theme);
		if (theme !== scheme) throw new Error(`${path}: expected data-theme=${scheme}, got ${theme}`);
		const { violations } = await new AxeBuilder({ page }).withTags(TAGS).analyze();
		for (const v of violations) {
			const key = `${path} ${scheme} ${width} ${v.id}`;
			found[key] = v.nodes.length;
			details[key] = { help: v.help, url: v.helpUrl, targets: v.nodes.map((n) => n.target.join(' ')) };
		}
	}
	await context.close();
}

try {
	await Promise.all(SCHEMES.flatMap((scheme) => WIDTHS.map((width) => scan(scheme, width))));
} finally {
	await browser.close();
	close();
}

if (update) {
	const sorted = Object.fromEntries(Object.entries(found).sort(([a], [b]) => a.localeCompare(b)));
	writeFileSync(BASELINE, JSON.stringify({ ...baseline, known: sorted }, null, '\t') + '\n');
	console.log(`Wrote ${Object.keys(sorted).length} known violation(s) to a11y-baseline.json.`);
	process.exit(0);
}

const regressions = [];
const improved = [];
for (const [key, count] of Object.entries(found)) {
	const known = baseline.known[key] ?? 0;
	if (count > known) regressions.push({ key, count, known });
	else if (count < known) improved.push(`${key}: ${count} (baseline ${known})`);
}
for (const [key, known] of Object.entries(baseline.known))
	if (!(key in found)) improved.push(`${key}: 0 (baseline ${known})`);

const runs = PAGES.length * SCHEMES.length * WIDTHS.length;
const lines = [
	'### Accessibility (axe, WCAG 2.2 A/AA)',
	'',
	`${PAGES.length} pages x ${SCHEMES.join('/')} x ${WIDTHS.join('/')} px = ${runs} scans. ` +
		`Violations: ${Object.keys(found).length} page/rule pairs, ${Object.keys(baseline.known).length} known in the baseline.`,
];
if (regressions.length) {
	lines.push('', '**New or increased violations:**', '');
	for (const r of regressions) {
		const d = details[r.key];
		lines.push(`- \`${r.key}\`: ${r.count} node(s), baseline ${r.known}. ${d.help} (${d.url})`);
		for (const t of d.targets.slice(0, 5)) lines.push(`  - \`${t}\``);
	}
}
if (improved.length)
	lines.push('', 'Fewer violations than the baseline (lower it with `node a11y.mjs --update-baseline`):', '', ...improved.map((i) => `- \`${i}\``));
const report = lines.join('\n');
console.log(report);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, report + '\n\n');
if (regressions.length) process.exit(1);
console.log('\nNo new accessibility violations.');
