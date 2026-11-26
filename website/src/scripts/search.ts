// Turn Pagefind results into the search dialog's items (Search.astro).
//
// A page result becomes one item: its title, its section, and its kind as
// the tag. When the match is in reference entries (flags, arguments, keys,
// errors, methods), those entries become items of their own, labelled
// "section · page", so `--unlock` or a pasted error message lands on its
// entry. Section and kind come from `data-pagefind-meta` (PageTitle.astro).

interface PagefindLocation {
	weight: number;
	balanced_score: number;
	location: number;
}
interface PagefindSubResult {
	title: string;
	url: string;
	anchor?: { id?: string; element?: string };
	locations: number[];
	weighted_locations?: PagefindLocation[];
}
export interface PagefindPage {
	url: string;
	meta: Record<string, string | undefined>;
	sub_results?: PagefindSubResult[];
}
export interface PagefindApi {
	options(options: Record<string, unknown>): Promise<void>;
	init(): Promise<void>;
	debouncedSearch(
		query: string,
		options?: Record<string, unknown>,
		ms?: number,
	): Promise<{ results: { data(): Promise<PagefindPage> }[] } | null>;
}
export interface SearchItem {
	title: string;
	url: string;
	meta: string;
	tag?: string;
}

/** Anchor id prefix -> the entry's tag. */
const ENTRY_TAGS: [string, string][] = [
	['flag-', 'Flag'],
	['arg-', 'Argument'],
	['key-', 'Key'],
	['err-', 'Error'],
	['method-', 'Method'],
];
const entryTag = (id: string | undefined) => ENTRY_TAGS.find(([p]) => id?.startsWith(p))?.[1];

const score = (sub: PagefindSubResult) =>
	sub.weighted_locations?.reduce((sum, l) => sum + l.balanced_score, 0) ?? sub.locations.length;

/** At most `limit` items, in Pagefind's page order. */
export function searchItems(pages: PagefindPage[], limit: number, perPage = 3): SearchItem[] {
	const items: SearchItem[] = [];
	for (const page of pages) {
		const title = page.meta.title ?? page.url;
		const section = page.meta.section ?? '';
		const entries = (page.sub_results ?? [])
			.filter((s) => entryTag(s.anchor?.id))
			.sort((a, b) => score(b) - score(a))
			.slice(0, perPage);
		if (entries.length) {
			for (const sub of entries)
				items.push({
					// An error heading starts with its "Error" tag.
					title: sub.anchor?.id?.startsWith('err-') ? sub.title.replace(/^Error\s*/, '') : sub.title,
					url: sub.url,
					meta: section ? `${section} · ${title}` : title,
					tag: entryTag(sub.anchor?.id),
				});
		} else {
			items.push({ title, url: page.url, meta: section, tag: page.meta.kind });
		}
		if (items.length >= limit) break;
	}
	return items.slice(0, limit);
}
