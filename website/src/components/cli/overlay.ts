import { getCollection, getEntry } from 'astro:content';
import { cliFor, type Cli, type OverlayCommand } from '../../data/cli';
import { snapshotOf } from '../../versions.mjs';

const cached = new Map<string, Record<string, OverlayCommand>>();

/**
 * The hand-written CLI text by command path, checked against the CLI data
 * once per build: main's overlay.yaml, or a version snapshot's frozen copy
 * (src/snapshots/<x.y>/cli-overlay.yaml).
 */
export async function getOverlay(locals: App.Locals): Promise<Record<string, OverlayCommand>> {
	const snapshot = snapshotOf(locals.starlightRoute?.entry.id ?? '');
	const key = snapshot?.slug ?? '';
	let map = cached.get(key);
	if (!map) {
		if (snapshot) {
			const entry = await getEntry('snapshotCliOverlay', snapshot.slug);
			if (!entry) throw new Error(`src/snapshots/${snapshot.slug}/cli-overlay.yaml is missing`);
			map = entry.data as Record<string, OverlayCommand>;
		} else {
			const entries = await getCollection('cliOverlay');
			map = Object.fromEntries(entries.map((e) => [e.id, e.data as OverlayCommand]));
		}
		const cli: Cli = cliFor(snapshot?.release);
		cli.checkOverlay(map);
		cached.set(key, map);
	}
	return map;
}
