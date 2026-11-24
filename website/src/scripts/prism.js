// Prism's interactive part (src/components/Prism.astro), ported from the
// stage 3 prototype (signature-2-prism.html rev 2) and its stage 4 link to
// the site SDK.
//
// - Every card renders its own language; CSS shows the site SDK's card.
//   The language radios set the site SDK (sdk.ts), and follow it.
// - Typing in the version never replaces the input being typed in: only the
//   other cards' inputs get the new value. One announcement, 900 ms after
//   typing stops.
// - Announcements use the page's one live region (live.ts).
import { SYS, canon, canonHtml, codeHtml, hexHtml, bandHtml, initial, langOf } from './prism-core.js';
import { onSdk, setSdk } from './sdk';
import { say as announce } from './live';

const RM = matchMedia('(prefers-reduced-motion: reduce)');
let sayT;
function say(msg, delay) {
	clearTimeout(sayT);
	sayT = setTimeout(() => announce(msg), delay || 0);
}
const q = (r, s) => r.querySelector(s);
const qa = (r, s) => [...r.querySelectorAll(s)];
function restart(el, cls) {
	if (!el || RM.matches) return;
	el.classList.remove(cls);
	void el.getBoundingClientRect();
	el.classList.add(cls);
}
function sha256(str) {
	const c = globalThis.crypto && crypto.subtle;
	if (!c) return Promise.reject(new Error('no SubtleCrypto'));
	return c.digest('SHA-256', new TextEncoder().encode(str)).then((d) =>
		Array.prototype.map.call(new Uint8Array(d), (x) => ('0' + x.toString(16)).slice(-2)).join(''),
	);
}

/** Make the Prism in `root` ([data-prism]) interactive. */
export function mountPrism(root) {
	if (root.dataset.pzReady) return;
	root.dataset.pzReady = '1';
	let st = initial(document.documentElement.dataset.sdk);
	let prevHex = '';
	let seq = 0;
	const cards = qa(root, '.pz-card');
	const canonEl = q(root, '[data-canon]');
	const hexEl = q(root, '[data-hex]');
	const bandEl = q(root, '[data-band]');
	const deltaEl = q(root, '[data-delta]');
	const bytesEl = q(root, '[data-bytes]');

	function render(focusInfo) {
		cards.forEach((c) => {
			const lang = c.dataset.lang;
			q(c, 'code').innerHTML = codeHtml(lang, st, true);
			qa(c, '[data-act]').forEach((b) => {
				const a = b.dataset.act;
				if (a === 'names') b.setAttribute('aria-pressed', String(!!st.n[lang]));
				if (a === 'fmt') b.setAttribute('aria-pressed', String(!!st.f[lang]));
			});
		});
		if (focusInfo) {
			const el = q(root, `.pz-card[data-lang="${focusInfo.lang}"] [data-slot="${focusInfo.slot}"]`);
			if (el) el.focus({ preventScroll: true });
		}
	}
	function syncVersion(src) {
		qa(root, '[data-slot="ver"]').forEach((el) => {
			if (el !== src) el.value = st.ver;
			const n = Math.max(st.ver.length, 3);
			el.size = n;
			el.style.width = n + 1.4 + 'ch';
		});
	}
	function update(kind, srcLang, keys, msg, focusInfo, wait) {
		if (focusInfo && focusInfo.src) syncVersion(focusInfo.src);
		else render(focusInfo);
		canonEl.innerHTML = canonHtml(st, kind === 'style' ? [] : keys);
		const s = canon(st);
		const my = ++seq;
		bytesEl.textContent = String(new TextEncoder().encode(s).length);
		sha256(s).then(
			(h) => {
				if (my !== seq) return;
				let diff = 0;
				for (let i = 0; i < 64; i++) if (h[i] !== prevHex[i]) diff++;
				hexEl.innerHTML = hexHtml(h);
				bandEl.innerHTML = bandHtml(h);
				let dk, dt;
				if (kind === 'init') {
					dk = '';
					dt = 'The same bytes give the same digest, in any language.';
				} else if (diff === 0) {
					dk = 'same';
					dt = 'Unchanged. Same bytes, so the same digest: 0 of 64 characters differ.';
				} else {
					dk = 'changed';
					dt = 'Changed. ' + diff + ' of 64 characters differ from the previous digest.';
				}
				deltaEl.dataset.k = dk;
				q(deltaEl, 'span').textContent = dt;
				if (kind !== 'init') {
					if (diff) restart(q(root, '.pz-digest'), 'flash');
					if (msg) say(msg + (diff ? ' Digest changed, now starts ' + h.slice(0, 8) + '.' : ' Digest unchanged, still ' + h.slice(0, 8) + '.'), wait);
				}
				prevHex = h;
			},
			() => {
				hexEl.textContent = "Not computed: this page needs HTTPS for the browser's SHA-256.";
				bandEl.innerHTML = '';
				if (kind !== 'init' && msg) say(msg, wait);
			},
		);
	}

	root.addEventListener('input', (e) => {
		const t = e.target;
		if (t.dataset.slot !== 'ver') return;
		const clean = t.value.replace(/[^0-9A-Za-z.+-]/g, '').slice(0, 20);
		if (clean !== t.value) {
			const c = t.selectionStart - (t.value.length - clean.length);
			t.value = clean;
			t.setSelectionRange(Math.max(0, c), Math.max(0, c));
		}
		st.ver = clean;
		const lang = t.closest('.pz-card').dataset.lang;
		update('value', lang, ['aliases'], 'Version set to ' + (clean || 'nothing') + ' in ' + langOf(lang).name + '.', { src: t }, 900);
	});
	root.addEventListener('click', (e) => {
		const t = e.target.closest('button');
		if (!t || !root.contains(t)) return;
		const card = t.closest('.pz-card');
		const lang = card ? card.dataset.lang : null;
		const L = lang ? langOf(lang).name : '';
		if (t.dataset.slot === 'inc') {
			st.docs = !st.docs;
			update('value', lang, ['includes'], (st.docs ? 'Added docs to the includes in ' : 'Removed docs from the includes in ') + L + '.', { lang, slot: 'inc' });
		} else if (t.dataset.act === 'names') {
			st.n[lang] = st.n[lang] ? 0 : 1;
			update('style', lang, [], L + ': variables ' + (st.n[lang] ? 'renamed.' : 'named as before.') + ' The definition did not change.', null);
		} else if (t.dataset.act === 'fmt') {
			st.f[lang] = st.f[lang] ? 0 : 1;
			update('style', lang, [], L + ': code ' + (st.f[lang] ? 'restyled: quotes, line breaks and call order.' : 'back to the first style.') + ' The definition did not change.', null);
		} else if (t.dataset.act === 'copy') {
			const s = canon(st);
			navigator.clipboard?.writeText(s).then(
				() => say('Copied the ' + new TextEncoder().encode(s).length + ' bytes of JSON.'),
				() => say('Copy failed. Select the text instead.'),
			);
		}
	});
	root.addEventListener('change', (e) => {
		const t = e.target;
		if (t.hasAttribute('data-target')) {
			st.target = +t.value;
			update('target', null, ['target'], 'Building on ' + SYS[st.target - 1] + '. The code is unchanged, but the target in the definition is.', null);
		} else if (t.name === 'pz-lang') {
			// The reader picked a language here: it becomes the site SDK.
			setSdk(t.value);
		}
	});
	// Follow the site SDK (the header select, a tab elsewhere, another tab).
	let first = true;
	onSdk((sdk) => {
		qa(root, 'input[name="pz-lang"]').forEach((r) => (r.checked = r.value === sdk));
		const changed = st.lang !== sdk;
		st.lang = sdk;
		if (!first && changed) {
			restart(q(root, `.pz-card[data-lang="${sdk}"]`), 'flash');
			say('Showing ' + langOf(sdk).name + '. Same definition, same digest.');
		}
		first = false;
	});
	update('init', null, [], '', null);
}
