// Print the median Lighthouse run per URL (from `lhci autorun`) as a table,
// and append it to the GitHub job summary.
import { appendFileSync, readFileSync } from 'node:fs';

const dir = new URL('./.lighthouseci/reports/', import.meta.url);
const manifest = JSON.parse(readFileSync(new URL('manifest.json', dir), 'utf8'));
const lines = [
	'### Lighthouse (mobile, lab, median of runs)',
	'',
	'Gates: CLS ≤ 0.05 and LCP ≤ 2.0 s fail the check. TBT ≤ 100 ms, Performance ≥ 95 and Accessibility 100 warn only.',
	'',
	'| Page | LCP | CLS | TBT | Performance | Accessibility |',
	'|---|---|---|---|---|---|',
];
for (const run of manifest.filter((/** @type {any} */ r) => r.isRepresentativeRun)) {
	const name = run.jsonPath.split(/[\\/]/).pop();
	const { audits } = JSON.parse(readFileSync(new URL(name, dir), 'utf8'));
	const path = new URL(run.url).pathname;
	const lcp = audits['largest-contentful-paint'].numericValue / 1000;
	const cls = audits['cumulative-layout-shift'].numericValue;
	const tbt = audits['total-blocking-time'].numericValue;
	lines.push(
		`| \`${path}\` | ${lcp.toFixed(2)} s${lcp > 2 ? ' ⚠️' : ''} | ${cls.toFixed(3)}${cls > 0.05 ? ' ❌' : ''} | ${Math.round(tbt)} ms | ${Math.round(run.summary.performance * 100)} | ${Math.round(run.summary.accessibility * 100)} |`,
	);
}
const report = lines.join('\n');
console.log(report);
if (process.env.GITHUB_STEP_SUMMARY) appendFileSync(process.env.GITHUB_STEP_SUMMARY, report + '\n\n');
