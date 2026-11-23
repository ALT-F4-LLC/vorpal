import { createHash } from 'node:crypto';

/** SHA-256 of a string, as lowercase hex. */
export function sha256(input: string): string {
	return createHash('sha256').update(input).digest('hex');
}

/**
 * Draw a sigil for a content digest: a four-fold blade, echoing the Vorpal
 * mark, whose blade lengths, angles and opacity come from the digest bytes.
 * The same content always yields the same sigil; any edit changes it.
 * Returns the inner SVG markup for a `viewBox="-50 -50 100 100"` canvas.
 */
export function sigilPaths(hex: string): string {
	const b = (i: number) => parseInt(hex.slice((i * 2) % 64, (i * 2) % 64 + 2), 16) / 255;
	const blades: string[] = [];
	for (let i = 0; i < 6; i++) {
		const v = b(i + 1);
		const w = b(i + 8);
		const d = 4 + i * 6.4;
		const len = 5 + v * 15;
		const tipX = len * 0.5;
		const backX = -len * 0.32 * w;
		const opacity = (0.35 + w * 0.6).toFixed(2);
		const shade = i % 3;
		blades.push(
			`<path class="s${shade}" fill-opacity="${opacity}" d="M0 ${-d}L${tipX.toFixed(2)} ${(-d - len).toFixed(2)}L${backX.toFixed(2)} ${(-d - len * 0.6).toFixed(2)}Z"/>`,
		);
	}
	const arm = blades.join('');
	const arms = [0, 90, 180, 270].map((r) => `<g transform="rotate(${r})">${arm}</g>`).join('');
	return `${arms}<circle class="core" r="3.2"/>`;
}
