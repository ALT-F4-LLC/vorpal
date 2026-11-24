// The page's one polite live region (design-system.md, accessibility rules).
//
// It lives in the header (Search.astro). A modal dialog makes the rest of
// the page inert, so while one is open the region moves into it, then back.

const region = () => document.getElementById('vp-live');
let home: { parent: Node; next: Node | null } | undefined;

/** Announce `message` politely. */
export function say(message: string): void {
	const live = region();
	if (!live) return;
	// Clear first, so the same words are announced again.
	live.textContent = '';
	setTimeout(() => (live.textContent = message), 50);
}

/** Move the region into an open modal `dialog`. */
export function adoptLive(dialog: HTMLElement): void {
	const live = region();
	if (!live || live.parentNode === dialog) return;
	if (!home && live.parentNode) home = { parent: live.parentNode, next: live.nextSibling };
	dialog.append(live);
}

/** Put the region back where it was. */
export function returnLive(): void {
	const live = region();
	if (!live || !home) return;
	home.parent.insertBefore(live, home.next);
	home = undefined;
}
