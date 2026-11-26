// @ts-check
// Expressive Code settings for every code block: Markdown fences and the
// <Code> component (used by SdkGroup). Kept here, not in astro.config.mjs,
// because <Code> can only read functions and plugins from this file.
import { defineEcConfig } from '@astrojs/starlight/expressive-code';
import { vorpalDark, vorpalLight } from './src/code-themes.mjs';
import cliBlocks from './src/plugins/ec-cli.mjs';

export default defineEcConfig({
	// CLI blocks (prompt), Expected output blocks and dashed placeholders.
	plugins: [cliBlocks()],
	// Built from the design's syntax tokens (src/code-themes.mjs). Light is
	// the base. Dark applies under <html class="vp-js" data-theme="dark">,
	// and, through the prefers-color-scheme block Expressive Code adds, also
	// when the system is dark and the page is not set to light. `.vp-js`
	// matters because Starlight's static HTML always says data-theme="dark":
	// without JavaScript the system setting decides (see tokens.css).
	themes: [vorpalLight, vorpalDark],
	themeCssSelector: (theme) => `.vp-js[data-theme='${theme.type}']`,
	useDarkModeMediaQuery: true,
	// The tokens are AA-tested (at least 5.4:1 on the code background), so
	// a 4.5 floor leaves them untouched; Expressive Code's default (5.5)
	// would re-tint some of them.
	minSyntaxHighlightingColorContrast: 4.5,
	styleOverrides: {
		borderRadius: 'var(--vp-r-lg)',
		borderColor: 'var(--vp-line)',
		frames: {
			editorBackground: 'var(--vp-code)',
			terminalBackground: 'var(--vp-code)',
			editorTabBarBackground: 'var(--vp-chrome)',
			editorTabBarBorderBottomColor: 'var(--vp-line)',
			editorActiveTabBackground: 'var(--vp-code)',
			editorActiveTabForeground: 'var(--vp-ink)',
			editorActiveTabIndicatorTopColor: 'var(--vp-accent-fill)',
			editorActiveTabIndicatorBottomColor: 'transparent',
			terminalTitlebarBackground: 'var(--vp-chrome)',
			terminalTitlebarForeground: 'var(--vp-ink-2)',
			terminalTitlebarBorderBottomColor: 'var(--vp-line)',
			terminalTitlebarDotsForeground: 'var(--vp-line-strong)',
			inlineButtonForeground: 'var(--vp-ink)',
			inlineButtonBorder: 'var(--vp-line-strong)',
			tooltipSuccessBackground: 'var(--vp-ok)',
			tooltipSuccessForeground: 'var(--vp-surface)',
			frameBoxShadowCssValue: 'none',
		},
	},
});
