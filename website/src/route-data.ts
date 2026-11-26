/**
 * Starlight route middleware for the page shell (docs-redesign stage 4,
 * decision 5). Registered as `routeMiddleware` in astro.config.mjs.
 *
 * - Keeps the whole sidebar tree, the sections and the page's "On this page"
 *   items in `locals.vorpalNav` for the header, Menu, breadcrumb, the
 *   disclosure under the title and the footer site map.
 * - Narrows Starlight's `sidebar` to the current section's pages, so its
 *   Sidebar components (aria-current, the state persister) work unchanged.
 * - Previous and Next stay only inside an ordered section (Get started);
 *   elsewhere Related replaces them.
 * - Wide pages (frontmatter `wide`) drop the right-hand "On this page"
 *   column; they always use the disclosure under the title.
 */
import { defineRouteMiddleware } from '@astrojs/starlight/route-data';
import { SEQUENTIAL_SECTIONS, buildNav, flatten, type SidebarEntry } from './nav';
import { SNAPSHOTS, snapshotGroupLabel, snapshotOf } from './versions.mjs';

const snapshotGroups = new Set(SNAPSHOTS.map(snapshotGroupLabel));

export const onRequest = defineRouteMiddleware((context) => {
	const route = context.locals.starlightRoute;
	// Version snapshots (src/versions.mjs): Starlight's sidebar holds main's
	// groups, then one group per snapshot. A snapshot's page sees only its
	// snapshot's tree; main's pages never see the snapshots.
	const snapshot = snapshotOf(route.entry.id);
	let tree: SidebarEntry[];
	if (snapshot) {
		const own = route.sidebar.find((e) => e.type === 'group' && e.label === snapshotGroupLabel(snapshot));
		tree = own?.type === 'group' ? own.entries : [];
	} else {
		tree = route.sidebar.filter((e) => !(e.type === 'group' && snapshotGroups.has(e.label)));
	}
	route.sidebar = tree;
	const nav = buildNav(tree);
	nav.snapshot = snapshot;
	nav.toc = route.toc?.items.filter((item) => item.slug !== '_top');
	context.locals.vorpalNav = nav;

	const group = nav.section?.group;
	if (group) route.sidebar = group.entries;

	if (group && nav.section && SEQUENTIAL_SECTIONS.has(nav.section.label)) {
		const links = flatten(group.entries);
		const i = links.findIndex((l) => l.isCurrent);
		route.pagination = {
			prev: route.entry.data.prev === false ? undefined : links[i - 1],
			next: route.entry.data.next === false ? undefined : links[i + 1],
		};
	} else {
		route.pagination = { prev: undefined, next: undefined };
	}

	if (route.entry.data.wide) route.toc = undefined;
});
