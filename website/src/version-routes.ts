/**
 * Which pages exist in main and in each version snapshot, by URL path
 * (`/concepts/artifacts/`), so the version switcher, the snapshot banner
 * and the link rewrite (src/middleware.ts) can map a page to the same page
 * in another version. See src/versions.mjs.
 */
import { getCollection } from 'astro:content';
import { SNAPSHOTS, mainPathOf, prefixOf, snapshotOf, type Snapshot } from './versions.mjs';

export interface VersionPages {
	main: Set<string>;
	/** Main-tree paths (`/concepts/`) of each snapshot's pages, by slug. */
	snapshots: Map<string, Set<string>>;
}

let cached: Promise<VersionPages> | undefined;

/** A collection id as a URL path: `index` -> `/`, `reference/cli` -> `/reference/cli/`. */
export const pathOfId = (id: string) => (id === 'index' ? '/' : `/${id.replace(/\/index$/, '')}/`);

export function versionPages(): Promise<VersionPages> {
	cached ??= getCollection('docs', (e) => !e.data.draft).then((entries) => {
		const out: VersionPages = { main: new Set(), snapshots: new Map(SNAPSHOTS.map((s) => [s.slug, new Set()])) };
		for (const e of entries) {
			const path = pathOfId(e.id);
			const s = snapshotOf(path);
			if (s) out.snapshots.get(s.slug)!.add(mainPathOf(path));
			else out.main.add(path);
		}
		return out;
	});
	return cached;
}

/** One version of the docs, for the switcher. */
export interface VersionLink {
	/** `main` or the snapshot's slug (`0.6`). */
	label: string;
	snapshot?: Snapshot;
	/** The same page in that version, or that version's home when it has none. */
	href: string;
	/** True when `href` is the same page, not the home. */
	samePage: boolean;
	current: boolean;
}

/** Main and every snapshot, each linking the page at `path` there. */
export async function versionLinks(path: string): Promise<VersionLink[]> {
	const pages = await versionPages();
	const here = snapshotOf(path);
	const page = mainPathOf(path);
	const link = (snapshot: Snapshot | undefined, has: Set<string>): VersionLink => {
		const samePage = has.has(page);
		return {
			label: snapshot ? snapshot.slug : 'main',
			snapshot,
			href: `${prefixOf(snapshot)}${samePage ? page : '/'}`,
			samePage,
			current: snapshot === here,
		};
	};
	return [link(undefined, pages.main), ...SNAPSHOTS.map((s) => link(s, pages.snapshots.get(s.slug)!))];
}
