import { getCollection } from 'astro:content';
import { checkOverlay, type OverlayCommand } from '../../data/cli';

let cached: Record<string, OverlayCommand> | undefined;

/** overlay.yaml by command path, checked against main.json once per build. */
export async function getOverlay(): Promise<Record<string, OverlayCommand>> {
	if (!cached) {
		const entries = await getCollection('cliOverlay');
		const map = Object.fromEntries(entries.map((e) => [e.id, e.data as OverlayCommand]));
		checkOverlay(map);
		cached = map;
	}
	return cached;
}
