// @ts-check
/**
 * Versioned docs (docs-redesign step 9; critique-engineering M3,
 * design-system.md §14).
 *
 * - The docs for main live at the site root and change on every push.
 * - A version snapshot is a frozen copy of the docs for one release line,
 *   served at `/v/<x.y>/`. Its pages are in src/content/docs/v/<x.y>/; the
 *   data its pages render (the SDK examples, the CLI overlay and the error
 *   entries) is in src/snapshots/<x.y>/; its CLI data is
 *   src/data/cli/releases/<x.y.z>.json. Nothing in a snapshot reads main's
 *   files, so later edits on main never change it.
 * - src/snapshots/versions.json lists the snapshots, newest first, each
 *   with the release it documents and its frozen sidebar.
 *   script/website-versions/snapshot.mjs writes all of it when a release is
 *   cut; nothing at build time writes to src/.
 *
 * This module is plain JavaScript so astro.config.mjs, the Markdown copies
 * integration and the page components share it.
 */
import pkg from '../package.json' with { type: 'json' };
import index from './snapshots/versions.json' with { type: 'json' };

/**
 * @typedef {import('./integrations/markdown-twins.mjs').SidebarItem} SidebarItem
 * @typedef {{ slug: string, release: string, sidebar: SidebarItem[] }} Snapshot
 */

/** The latest release (website/package.json). */
export const LATEST_RELEASE = pkg.version;

/** URL segment that holds the snapshots: `/v/<x.y>/`. */
export const VERSION_DIR = 'v';

/** Every snapshot, newest first. @type {Snapshot[]} */
export const SNAPSHOTS = index.versions;

/** The snapshot of the latest release, when it has one. */
export const LATEST_SNAPSHOT = SNAPSHOTS.find((s) => s.release === LATEST_RELEASE);

/** `0.6.0` -> `0.6`. @param {string} release */
export const releaseLine = (release) => release.split('.').slice(0, 2).join('.');

/** The label of a snapshot's sidebar group in Starlight's config. @param {Snapshot} s */
export const snapshotGroupLabel = (s) => `${VERSION_DIR}/${s.slug}`;

/**
 * The snapshot a collection id (`v/0.6/concepts/artifacts`) or a URL path
 * (`/v/0.6/concepts/artifacts/`) belongs to; undefined for main.
 * @param {string} idOrPath
 */
export function snapshotOf(idOrPath) {
	const m = /^\/?v\/([^/]+)(?:\/|$)/.exec(idOrPath);
	return m ? SNAPSHOTS.find((s) => s.slug === m[1]) : undefined;
}

/** The URL prefix of a snapshot, `/v/0.6`, or '' for main. @param {Snapshot | undefined} s */
export const prefixOf = (s) => (s ? `/${VERSION_DIR}/${s.slug}` : '');

/**
 * A page's path in main's tree: `/v/0.6/concepts/` -> `/concepts/`.
 * @param {string} path
 */
export function mainPathOf(path) {
	const s = snapshotOf(path);
	return s ? path.slice(prefixOf(s).length) || '/' : path;
}

/**
 * The sidebar config of a snapshot, with its slugs under `v/<x.y>/`.
 * @param {Snapshot} s
 * @returns {SidebarItem[]}
 */
export function snapshotSidebar(s) {
	/** @param {SidebarItem} item @returns {SidebarItem} */
	const move = (item) => ({
		...item,
		...(item.slug !== undefined ? { slug: `${VERSION_DIR}/${s.slug}/${item.slug}`.replace(/\/$/, '') } : {}),
		...(item.link?.startsWith('/') ? { link: `${prefixOf(s)}${item.link}` } : {}),
		...(item.items ? { items: item.items.map(move) } : {}),
	});
	return s.sidebar.map(move);
}

/**
 * Fail the build when the latest release has no snapshot, so a release
 * cannot ship docs that readers of the previous release lose.
 */
export function checkSnapshots() {
	if (!LATEST_SNAPSHOT) {
		throw new Error(
			`The latest release ${LATEST_RELEASE} (website/package.json) has no version snapshot. ` +
				'Run `bun ../script/website-versions/snapshot.mjs` in website/ (see src/versions.mjs).',
		);
	}
	for (const s of SNAPSHOTS) {
		if (releaseLine(s.release) !== s.slug)
			throw new Error(`src/snapshots/versions.json: ${s.slug} documents ${s.release}, which is not in its line`);
	}
}
