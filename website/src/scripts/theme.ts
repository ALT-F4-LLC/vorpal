// The colour theme: Auto, Light or Dark (design-system.md, Colour).
//
// Same storage as Starlight, so its head script (ThemeProvider) applies the
// choice before first paint: `localStorage["starlight-theme"]` holds `light`
// or `dark`, and `auto` is stored as ''. Other open tabs follow a change.
// The header's segmented control (ThemeSelect.astro) and the Menu's radios
// (Header.astro) both go through here.

export type Theme = 'auto' | 'light' | 'dark';

const KEY = 'starlight-theme';

const parse = (value: unknown): Theme =>
	value === 'light' || value === 'dark' ? value : 'auto';

export function storedTheme(): Theme {
	try {
		return parse(localStorage.getItem(KEY));
	} catch {
		return 'auto';
	}
}

const systemTheme = (): 'light' | 'dark' =>
	matchMedia('(prefers-color-scheme: light)').matches ? 'light' : 'dark';

/** Apply `theme` to the page and tell the controls. */
function apply(theme: Theme): void {
	document.documentElement.dataset.theme = theme === 'auto' ? systemTheme() : theme;
	document.dispatchEvent(new CustomEvent('vorpal:theme', { detail: { theme } }));
}

/** The reader picked a theme: store it and apply it. */
export function setTheme(value: string): void {
	const theme = parse(value);
	try {
		localStorage.setItem(KEY, theme === 'auto' ? '' : theme);
	} catch {}
	apply(theme);
}

/** Call `fn` with the chosen theme now and on every change. */
export function onTheme(fn: (theme: Theme) => void): void {
	fn(storedTheme());
	document.addEventListener('vorpal:theme', (e) => {
		fn(parse((e as CustomEvent<{ theme: unknown }>).detail?.theme));
	});
}

matchMedia('(prefers-color-scheme: light)').addEventListener('change', () => {
	if (storedTheme() === 'auto') apply('auto');
});

window.addEventListener('storage', (event) => {
	if (event.key === KEY || event.key === null) apply(storedTheme());
});
