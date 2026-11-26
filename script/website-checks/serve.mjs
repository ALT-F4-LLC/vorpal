// A minimal static server for website/dist, shared by the checks.
// Serves `/path/` as `/path/index.html` and sends 404.html for missing files,
// like Cloudflare Pages.
import { createReadStream, existsSync, statSync } from 'node:fs';
import { createServer } from 'node:http';
import { extname, join, normalize, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const DIST = resolve(
	process.env.WEBSITE_DIST ?? fileURLToPath(new URL('../../website/dist', import.meta.url)),
);

const TYPES = {
	'.css': 'text/css; charset=utf-8',
	'.html': 'text/html; charset=utf-8',
	'.ico': 'image/x-icon',
	'.js': 'text/javascript; charset=utf-8',
	'.json': 'application/json',
	'.md': 'text/markdown; charset=utf-8',
	'.mjs': 'text/javascript; charset=utf-8',
	'.png': 'image/png',
	'.svg': 'image/svg+xml',
	'.txt': 'text/plain; charset=utf-8',
	'.wasm': 'application/wasm',
	'.woff2': 'font/woff2',
	'.xml': 'application/xml',
};

/** @returns {Promise<{ origin: string, close: () => void }>} */
export function serve(root = DIST) {
	const server = createServer((req, res) => {
		let path = decodeURIComponent(new URL(req.url ?? '/', 'http://x').pathname);
		if (path.endsWith('/')) path += 'index.html';
		let file = normalize(join(root, path));
		let status = 200;
		if (!file.startsWith(root) || !existsSync(file) || !statSync(file).isFile()) {
			file = join(root, '404.html');
			status = 404;
		}
		res.writeHead(status, { 'content-type': TYPES[extname(file)] ?? 'application/octet-stream' });
		createReadStream(file).pipe(res);
	});
	return new Promise((done) => {
		server.listen(0, '127.0.0.1', () => {
			const { port } = /** @type {import('node:net').AddressInfo} */ (server.address());
			done({ origin: `http://127.0.0.1:${port}`, close: () => server.close() });
		});
	});
}
