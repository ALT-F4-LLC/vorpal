#!/usr/bin/env node
// Freeze the docs for the latest release into a version snapshot
// (docs-redesign step 9; website/src/versions.mjs).
//
// Run it when a release is cut, after website/package.json has the new
// version and before main's docs move on:
//
//   cd website && bun ../script/website-versions/snapshot.mjs
//
// For release x.y.z it writes, under website/src:
// - content/docs/v/x.y/: every page of the docs for main except the 404 page
//   and drafts. MDX imports are re-pointed from the new folder; `related`
//   frontmatter ids move under v/x.y/; each page gets its `slug`. Links stay as written: the site
//   rewrites them for the snapshot when it renders (src/middleware.ts).
// - snapshots/x.y/: the data those pages render, frozen: examples/ (the SDK
//   examples), cli-overlay.yaml and errors/ (the error entries).
// - data/cli/releases/x.y.z.json, from main.json, when it is missing.
// - snapshots/versions.json: the snapshot's entry and frozen sidebar.
//
// A patch release (x.y.1) replaces the x.y snapshot. Uses Node built-ins
// only, so Bun or Node runs it. The output depends only on the files in
// website/src, so running it twice gives the same result.
import { cp, mkdir, readFile, readdir, rm, writeFile } from 'node:fs/promises';
import { existsSync } from 'node:fs';
import { dirname, join, relative, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';

const WEBSITE = resolve(dirname(fileURLToPath(import.meta.url)), '../../website');
const SRC = join(WEBSITE, 'src');
const DOCS = join(SRC, 'content/docs');
const VERSION_DIR = 'v';

const pkg = JSON.parse(await readFile(join(WEBSITE, 'package.json'), 'utf8'));
const release = String(pkg.version);
if (!/^\d+\.\d+\.\d+$/.test(release)) throw new Error(`website/package.json: "${release}" is not a release version`);
const slug = release.split('.').slice(0, 2).join('.');
const pagesOut = join(DOCS, VERSION_DIR, slug);
const dataOut = join(SRC, 'snapshots', slug);

/** Paths under content/docs that a snapshot leaves out. */
const skip = (/** @type {string} */ rel) =>
	rel.startsWith(`${VERSION_DIR}/`) || rel === '404.mdx' || rel === '404.md' || rel.startsWith('specimens/');

const posix = (/** @type {string} */ p) => p.split(sep).join('/');

/** True when the page's frontmatter has `draft: true`. */
const isDraft = (/** @type {string} */ text) => /^---\n[\s\S]*?^draft:\s*true\s*$[\s\S]*?^---$/m.test(text);

/**
 * Re-point relative import specifiers from `from` (the page's old path) to
 * `to` (its path in the snapshot). Targets inside the copied pages keep
 * pointing at the copy; everything else (components, data) at the same file.
 */
function moveImports(/** @type {string} */ text, /** @type {string} */ from, /** @type {string} */ to) {
	// Only the ESM block after the frontmatter: code samples further down
	// hold `import` lines too.
	const fmEnd = text.startsWith('---\n') ? text.indexOf('\n---', 4) + 4 : 0;
	const lines = text.slice(fmEnd).split('\n');
	let n = 0;
	while (n < lines.length && (/^\s*$/.test(lines[n]) || /^import\s/.test(lines[n]))) n += 1;
	const head = text.slice(0, fmEnd) + lines.slice(0, n).join('\n');
	const rest = text.slice(head.length);
	return rewriteImports(head, from, to) + rest;
}

function rewriteImports(/** @type {string} */ text, /** @type {string} */ from, /** @type {string} */ to) {
	return text.replace(/^(import\b[^'"\n]*?(?:from\s*)?['"])(\.{1,2}\/[^'"]+)(['"])/gm, (m, head, spec, tail) => {
		let target = resolve(dirname(from), spec);
		const rel = posix(relative(DOCS, target));
		if (!rel.startsWith('..') && !skip(rel)) target = join(pagesOut, rel);
		let out = posix(relative(dirname(to), target));
		if (!out.startsWith('.')) out = `./${out}`;
		return `${head}${out}${tail}`;
	});
}

/** Move `related:` ids (a YAML list in the frontmatter) under v/x.y/. */
function moveRelated(/** @type {string} */ text) {
	const end = text.indexOf('\n---', 4);
	if (!text.startsWith('---\n') || end < 0) return text;
	const fm = text.slice(0, end).replace(/^related:\n((?:\s+-\s+.+\n?)+)/m, (block) =>
		block.replace(/^(\s+-\s+)(['"]?)([^'"\n]+)\2\s*$/gm, (m, dash, q, id) => `${dash}${q}${VERSION_DIR}/${slug}/${id}${q}`),
	);
	return fm + text.slice(end);
}

/** Pages still marked `unreleased` on main: the release makes them released. */
/** @type {string[]} */
const stillUnreleased = [];

// 1. Pages.
await rm(pagesOut, { recursive: true, force: true });
let pages = 0;
for (const entry of (await readdir(DOCS, { recursive: true, withFileTypes: true })).sort((a, b) =>
	join(a.parentPath, a.name).localeCompare(join(b.parentPath, b.name)),
)) {
	if (!entry.isFile()) continue;
	const file = join(entry.parentPath, entry.name);
	const rel = posix(relative(DOCS, file));
	if (skip(rel)) continue;
	const out = join(pagesOut, rel);
	await mkdir(dirname(out), { recursive: true });
	if (!/\.mdx?$/.test(rel)) {
		await cp(file, out);
		continue;
	}
	const text = await readFile(file, 'utf8');
	if (isDraft(text)) continue;
	// Starlight's ids drop the dot of `0.6` (`v/06/...`), so each copy names
	// its own slug.
	const pageSlug = `${VERSION_DIR}/${slug}/${rel.replace(/\.mdx?$/, '').replace(/(^|\/)index$/, '')}`.replace(/\/$/, '');
	if (!text.startsWith('---\n') || /^slug:/m.test(text.slice(0, text.indexOf('\n---', 4))))
		throw new Error(`${rel}: a page needs frontmatter and no \`slug\` of its own`);
	let copy = text.replace(/^---\n/, `---\nslug: ${pageSlug}\n`);
	copy = moveRelated(copy);
	// A release has nothing unreleased in it.
	copy = copy.replace(/^(---\n[\s\S]*?)^unreleased:.*\n/m, (m, head) => {
		stillUnreleased.push(rel);
		return head;
	});
	if (rel.endsWith('.mdx')) copy = moveImports(copy, file, out);
	await writeFile(out, copy);
	pages += 1;
}

// 2. Data the pages render.
await rm(dataOut, { recursive: true, force: true });
await mkdir(dataOut, { recursive: true });
await cp(join(SRC, 'examples'), join(dataOut, 'examples'), { recursive: true });
await cp(join(SRC, 'data/cli/overlay.yaml'), join(dataOut, 'cli-overlay.yaml'));
await mkdir(join(dataOut, 'errors'));
for (const name of (await readdir(join(SRC, 'content/errors'))).filter((f) => f.endsWith('.yaml')).sort())
	await cp(join(SRC, 'content/errors', name), join(dataOut, 'errors', name));

// 3. The release's CLI data.
const releaseJson = join(SRC, 'data/cli/releases', `${release}.json`);
if (!existsSync(releaseJson)) await cp(join(SRC, 'data/cli/main.json'), releaseJson);

// 4. The index, newest first.
const { sidebar } = await import(pathToFileURL(join(SRC, 'sidebar.mjs')).href);
const indexFile = join(SRC, 'snapshots/versions.json');
const index = existsSync(indexFile) ? JSON.parse(await readFile(indexFile, 'utf8')) : { versions: [] };
const semver = (/** @type {string} */ v) => v.split('.').map(Number);
index.versions = [
	...index.versions.filter((/** @type {{ slug: string }} */ v) => v.slug !== slug),
	{ slug, release, sidebar },
].sort((/** @type {{ release: string }} */ a, /** @type {{ release: string }} */ b) => {
	const [x, y] = [semver(a.release), semver(b.release)];
	for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return y[i] - x[i];
	return 0;
});
index.$comment =
	'Version snapshots, newest first (src/versions.mjs). Written by script/website-versions/snapshot.mjs; do not edit by hand.';
const ordered = { $comment: index.$comment, versions: index.versions };
await writeFile(indexFile, `${JSON.stringify(ordered, null, '\t')}\n`);

if (stillUnreleased.length)
	console.warn(
		`These pages say \`unreleased: true\`, but ${release} releases them; remove it on main:\n  ${stillUnreleased.join('\n  ')}`,
	);
console.log(
	`Snapshot ${slug} (release ${release}): ${pages} pages in ${posix(relative(WEBSITE, pagesOut))}, data in ${posix(relative(WEBSITE, dataOut))}.`,
);
