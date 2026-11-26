// Reference data checks (docs-redesign step 5). Run with Bun from the repo
// root or this folder: `bun script/website-checks/reference.mjs [--fix]`.
// Needs no install: it uses Bun's built-in YAML parser.
//
// 1. Error messages: every `sources[].match` in website/src/content/errors/
//    must still be in the Rust or SDK file it names, so the Error reference
//    never quotes a message the code no longer prints. A `line` that moved
//    is a warning with the new line; `--fix` rewrites it.
// 2. Version marks: prints what the generated CLI data (main.json) adds,
//    removes or changes against the latest release's data, which drives the
//    Unreleased and Since marks on the CLI pages. The latest release (the
//    version in website/package.json) must have a data file.
//
// The generated data itself is checked against the CLI by a cargo test
// (cli/src/command/cli_reference.rs), which needs Rust; the overlay is
// checked against main.json by the website build.
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const ERRORS = join(ROOT, 'website/src/content/errors');
const CLI = join(ROOT, 'website/src/data/cli');
const fix = process.argv.includes('--fix');
const gh = !!process.env.GITHUB_ACTIONS;

let failed = 0;
let warned = 0;
const out = [];

// 1. Error messages.
out.push('### Error reference: messages in the source', '');
for (const name of readdirSync(ERRORS).filter((f) => f.endsWith('.yaml')).sort()) {
	const path = join(ERRORS, name);
	let text = readFileSync(path, 'utf8');
	const data = Bun.YAML.parse(text);
	for (const src of data.sources ?? []) {
		const file = join(ROOT, src.file);
		const where = `website/src/content/errors/${name}`;
		if (!existsSync(file)) {
			failed++;
			out.push(`- FAIL \`${name}\`: ${src.file} does not exist`);
			if (gh) console.log(`::error file=${where}::${src.file} does not exist`);
			continue;
		}
		const lines = readFileSync(file, 'utf8').split('\n');
		const hits = lines.flatMap((l, i) => (l.includes(src.match) ? [i + 1] : []));
		if (!hits.length) {
			failed++;
			out.push(`- FAIL \`${name}\`: "${src.match}" is no longer in ${src.file}`);
			if (gh) console.log(`::error file=${where}::"${src.match}" is no longer in ${src.file}: update the entry`);
		} else if (!hits.includes(src.line)) {
			const best = hits.reduce((a, b) => (Math.abs(b - src.line) < Math.abs(a - src.line) ? b : a));
			if (fix) {
				text = text.replace(
					new RegExp(`(file: ${src.file.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\n\\s+line: )${src.line}\\b`),
					`$1${best}`,
				);
				out.push(`- fixed \`${name}\`: ${src.file}:${src.line} -> ${best}`);
			} else {
				warned++;
				out.push(`- warn \`${name}\`: "${src.match}" moved to ${src.file}:${best} (was ${src.line}); run with --fix`);
				if (gh) console.log(`::warning file=${where}::${src.file}:${src.line} moved to line ${best}`);
			}
		}
	}
	if (fix) writeFileSync(path, text);
}
if (!failed && !warned) out.push('Every message is in its source file at the line given.');

// 2. Version marks.
const pkg = JSON.parse(readFileSync(join(ROOT, 'website/package.json'), 'utf8'));
const main = JSON.parse(readFileSync(join(CLI, 'main.json'), 'utf8'));
const releaseFile = join(CLI, 'releases', `${pkg.version}.json`);
out.push('', `### CLI reference: main against ${pkg.version}`, '');
if (!existsSync(releaseFile)) {
	failed++;
	out.push(`- FAIL website/src/data/cli/releases/${pkg.version}.json is missing: copy main.json from the ${pkg.version} tag.`);
} else {
	const release = JSON.parse(readFileSync(releaseFile, 'utf8'));
	const flat = (data) => {
		const m = new Map();
		for (const g of data.global) m.set(`vorpal ${g.long ? `--${g.long}` : g.id} (global)`, g);
		for (const c of data.commands) {
			m.set(`vorpal ${c.path}`, { about: c.about });
			for (const a of c.arguments) m.set(`vorpal ${c.path} <${a.value_name}>`, a);
			for (const o of c.options) m.set(`vorpal ${c.path} --${o.long}`, o);
		}
		return m;
	};
	const [now, then] = [flat(main), flat(release)];
	const lines = [];
	for (const [k, v] of now) {
		if (!then.has(k)) lines.push(`- Unreleased (new): \`${k}\``);
		else if (JSON.stringify(v.default) !== JSON.stringify(then.get(k).default))
			lines.push(`- Changed default: \`${k}\` ${JSON.stringify(then.get(k).default)} -> ${JSON.stringify(v.default)}`);
	}
	for (const k of then.keys()) if (!now.has(k)) lines.push(`- Removed on main: \`${k}\``);
	out.push(...(lines.length ? lines : [`No CLI changes since ${pkg.version}: no Unreleased marks.`]));
}

const report = out.join('\n');
console.log(report);
if (process.env.GITHUB_STEP_SUMMARY) writeFileSync(process.env.GITHUB_STEP_SUMMARY, `${report}\n`, { flag: 'a' });
process.exit(failed ? 1 : 0);
