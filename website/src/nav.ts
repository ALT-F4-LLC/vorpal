/**
 * Site navigation for the page shell (docs-redesign stage 4, decision 5).
 *
 * The header shows six top-level sections. A section is a top-level group of
 * Starlight's `sidebar` config (astro.config.mjs), matched by label. Sections
 * with no pages yet link to the nearest existing page instead (`fallback`)
 * and are never current.
 *
 * `route-data.ts` (Starlight route middleware) uses these helpers to
 * narrow the sidebar to the current section and keep the full tree in
 * `Astro.locals.vorpalNav` for the header, Menu, breadcrumb and site map.
 */
import type { StarlightRouteData } from '@astrojs/starlight/route-data';

export type SidebarEntry = StarlightRouteData['sidebar'][number];
export type SidebarGroup = Extract<SidebarEntry, { type: 'group' }>;
export type SidebarLink = Extract<SidebarEntry, { type: 'link' }>;
export type TocItem = NonNullable<StarlightRouteData['toc']>['items'][number];

interface SectionConfig {
	label: string;
	/** The sidebar group this section shows. */
	group?: string;
	/** Where the section links while it has no pages of its own. */
	fallback?: string;
}

export const SECTIONS: SectionConfig[] = [
	{ label: 'Get started', group: 'Get started' },
	{ label: 'Guides', group: 'Guides' },
	{ label: 'Reference', group: 'Reference' },
	{ label: 'Troubleshooting', fallback: '/getting-started/installation/#troubleshooting' },
	{ label: 'Concepts', group: 'Concepts' },
	{ label: 'Self-hosting', fallback: '/concepts/architecture/#services-in-detail' },
];

/** Sections whose pages form an ordered sequence keep Previous and Next. */
export const SEQUENTIAL_SECTIONS = new Set(['Get started']);

export interface Section {
	label: string;
	href: string;
	current: boolean;
	group?: SidebarGroup;
}

export interface VorpalNav {
	/** The whole sidebar tree, before it is narrowed to the current section. */
	sidebar: SidebarEntry[];
	sections: Section[];
	/** The current section, when the page belongs to one. */
	section?: Section;
	/** Groups between the section and the page, for the breadcrumb. */
	trail: SidebarGroup[];
	/** The page's "On this page" items, kept for the disclosure under the title. */
	toc?: TocItem[];
}

export function flatten(entries: SidebarEntry[]): SidebarLink[] {
	return entries.flatMap((e) => (e.type === 'link' ? [e] : flatten(e.entries)));
}

/** The groups from the top of the tree down to the current page. */
function pathToCurrent(entries: SidebarEntry[]): SidebarGroup[] | undefined {
	for (const e of entries) {
		if (e.type === 'link') {
			if (e.isCurrent) return [];
		} else {
			const below = pathToCurrent(e.entries);
			if (below) return [e, ...below];
		}
	}
	return undefined;
}

export function buildNav(sidebar: SidebarEntry[]): VorpalNav {
	const path = pathToCurrent(sidebar) ?? [];
	const sections = SECTIONS.map((s): Section => {
		const group = s.group
			? sidebar.find((e): e is SidebarGroup => e.type === 'group' && e.label === s.group)
			: undefined;
		const first = group ? flatten(group.entries)[0] : undefined;
		return {
			label: s.label,
			href: first?.href ?? s.fallback ?? '/',
			current: !!group && path[0] === group,
			group,
		};
	});
	return {
		sidebar,
		sections,
		section: sections.find((s) => s.current),
		trail: path.slice(1),
	};
}
