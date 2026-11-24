// The site-wide SDK choice (docs-redesign stage 4, decision 3).
//
// One choice for every example group on every page: Python by default,
// stored in `localStorage["vorpal-sdk"]`, and settable with `?sdk=`. The
// inline head script (SDK_HEAD_SCRIPT, run by Head.astro before first paint)
// sets <html data-sdk>, and CSS (components.css) shows only that SDK's
// panels, so nothing shifts. This module keeps the controls (header select,
// Menu radios, example tabs) in step and tells other code through a
// `vorpal:sdk` event on `document`.

import { DEFAULT_SDK, SDK_KEY, isSdk, type Sdk } from './sdk-head';

export { SDKS, isSdk, type Sdk } from './sdk-head';

/** The SDK shown now (what the head script chose). */
export function currentSdk(): Sdk {
	const sdk = document.documentElement.dataset.sdk;
	return isSdk(sdk) ? sdk : DEFAULT_SDK;
}

function storedSdk(): Sdk {
	try {
		const sdk = localStorage.getItem(SDK_KEY);
		if (isSdk(sdk)) return sdk;
	} catch {}
	return DEFAULT_SDK;
}

/** Show `sdk` everywhere and announce it; store it when the reader chose it. */
export function setSdk(sdk: string, store = true): void {
	if (!isSdk(sdk)) return;
	document.documentElement.dataset.sdk = sdk;
	if (store) {
		try {
			localStorage.setItem(SDK_KEY, sdk);
		} catch {}
	}
	document.dispatchEvent(new CustomEvent('vorpal:sdk', { detail: { sdk } }));
}

/** Call `fn` now and on every change, from this tab or another. */
export function onSdk(fn: (sdk: Sdk) => void): void {
	fn(currentSdk());
	document.addEventListener('vorpal:sdk', (e) => {
		const sdk = (e as CustomEvent<{ sdk: unknown }>).detail?.sdk;
		if (isSdk(sdk)) fn(sdk);
	});
}

// Another open tab changed the choice. Registered once, however many
// modules import this file (Vite bundles it into one shared chunk).
window.addEventListener('storage', (event) => {
	if (event.key === SDK_KEY || event.key === null) setSdk(storedSdk(), false);
});

/**
 * Wire up the example groups (SdkGroup.astro): tab panels, manual
 * activation, and keeping the clicked tab in place on screen.
 */
export function initSdkGroups(): void {
	const groups = document.querySelectorAll<HTMLElement>('[data-vp-sdkg]');
	if (!groups.length) return;
	groups.forEach((group) => {
		const tabs = [...group.querySelectorAll<HTMLButtonElement>('[role="tab"]')];
		// Tab panels exist only with JavaScript; without it they are plain
		// sections under their headings.
		group.querySelectorAll<HTMLElement>('[data-sdk-panel]').forEach((panel) => {
			panel.setAttribute('role', 'tabpanel');
			panel.setAttribute('aria-labelledby', panel.dataset.tab!);
		});
		tabs.forEach((tab, i) => {
			tab.addEventListener('click', () => {
				// Keep the clicked tab where it is on screen, although panels above
				// it (in other groups) change height.
				const before = tab.getBoundingClientRect().top;
				setSdk(tab.dataset.sdk!);
				const moved = tab.getBoundingClientRect().top - before;
				if (moved) window.scrollBy({ top: moved, behavior: 'instant' });
			});
			// Manual activation: arrows, Home and End move focus only.
			tab.addEventListener('keydown', (e) => {
				const to =
					e.key === 'ArrowRight' ? (i + 1) % tabs.length
					: e.key === 'ArrowLeft' ? (i - 1 + tabs.length) % tabs.length
					: e.key === 'Home' ? 0
					: e.key === 'End' ? tabs.length - 1
					: -1;
				if (to < 0) return;
				e.preventDefault();
				tabs[to].focus();
			});
		});
	});
	onSdk((sdk) =>
		groups.forEach((group) =>
			group.querySelectorAll<HTMLButtonElement>('[role="tab"]').forEach((tab) => {
				const selected = tab.dataset.sdk === sdk;
				tab.setAttribute('aria-selected', String(selected));
				tab.tabIndex = selected ? 0 : -1;
			}),
		),
	);
}
