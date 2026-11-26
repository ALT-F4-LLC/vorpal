// Constants for the SDK choice, safe to import on the server (Head.astro
// inlines SDK_HEAD_SCRIPT). The browser side is sdk.ts.

export const SDK_KEY = 'vorpal-sdk';
export const SDKS = ['python', 'typescript', 'go', 'rust'] as const;
export type Sdk = (typeof SDKS)[number];
export const DEFAULT_SDK: Sdk = 'python';

export const isSdk = (value: unknown): value is Sdk =>
	typeof value === 'string' && (SDKS as readonly string[]).includes(value);

/**
 * Inline, not a module, and first in <head>: `?sdk=` wins (and is stored),
 * then the stored choice, then Python. Kept as a string so Head.astro can
 * inline it.
 */
export const SDK_HEAD_SCRIPT = `(() => {
	const sdks = ${JSON.stringify(SDKS)};
	let sdk = null;
	try { sdk = new URLSearchParams(location.search).get('sdk'); } catch {}
	if (sdks.includes(sdk)) {
		try { localStorage.setItem('${SDK_KEY}', sdk); } catch {}
	} else {
		try { sdk = localStorage.getItem('${SDK_KEY}'); } catch {}
	}
	document.documentElement.dataset.sdk = sdks.includes(sdk) ? sdk : '${DEFAULT_SDK}';
})();`;

