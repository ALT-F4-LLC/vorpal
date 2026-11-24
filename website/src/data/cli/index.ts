/**
 * The generated CLI reference (docs-redesign step 5; critique-engineering
 * row G).
 *
 * - `main.json` is generated from the clap definitions in cli/src by a cargo
 *   test (cli/src/command/cli_reference.rs), which also fails CI when the
 *   file and the CLI differ. It is committed, so the site builds with Bun
 *   alone.
 * - `releases/<version>.json` is the same data for each release, taken at
 *   its tag. When a release is cut, copy main.json to
 *   releases/<version>.json. The latest release must have a file (the
 *   version in website/package.json); the build fails otherwise.
 * - `overlay.yaml` (the `cliOverlay` collection) holds the hand-written text.
 *
 * Version marks (final-design decision 7, team decision 4): a command,
 * argument or flag that is in main.json but not in the latest release is
 * Unreleased. One that first appears in a later release than the oldest
 * release file shows "Since <version>". Anything already in the oldest file
 * shows no mark, because older releases have no data.
 */
import pkg from '../../../package.json';
import main from './main.json';

export { blocks, inline } from '../inline';

export interface CliArg {
	id: string;
	long: string | null;
	short: string | null;
	value_name: string | null;
	action: 'flag' | 'flag-false' | 'append' | 'count' | 'set';
	required: boolean;
	global: boolean;
	default: string | string[] | null;
	optional_value: boolean;
	require_equals: boolean;
	env: string | null;
	possible_values: string[] | null;
	conflicts_with: string[];
	trailing: boolean;
	help: string | null;
	long_help: string | null;
}

export interface CliCommand {
	path: string;
	about: string | null;
	long_about: string | null;
	arguments: CliArg[];
	options: CliArg[];
	subcommands: string[];
}

export interface CliData {
	name: string;
	version: string;
	placeholders: Record<string, string>;
	global: CliArg[];
	commands: CliCommand[];
}

export interface OverlayItem {
	text: string;
	value_name?: string;
	default?: string;
	setting?: string;
	env?: string;
	why?: string;
	since?: string;
}

export interface OverlayCommand {
	summary?: string;
	arguments?: Record<string, OverlayItem>;
	options?: Record<string, OverlayItem>;
}

export const cli = main as CliData;

/** The pseudo command that holds the global options in overlay.yaml. */
export const GLOBAL = '_global';

const releaseFiles = import.meta.glob<CliData>('./releases/*.json', { eager: true, import: 'default' });

const semver = (v: string) => v.split('.').map(Number);
const cmp = (a: string, b: string) => {
	const [x, y] = [semver(a), semver(b)];
	for (let i = 0; i < 3; i++) if (x[i] !== y[i]) return x[i] - y[i];
	return 0;
};

/** Release data, oldest first. */
export const releases = Object.entries(releaseFiles)
	.map(([file, data]) => ({ version: file.replace(/^.*\/|\.json$/g, ''), data }))
	.sort((a, b) => cmp(a.version, b.version));

export const latestRelease = pkg.version;
if (!releases.some((r) => r.version === latestRelease)) {
	throw new Error(
		`src/data/cli/releases/${latestRelease}.json is missing: copy main.json from the ${latestRelease} tag (see src/data/cli/index.ts).`,
	);
}

/** Every command, argument and option in a data file, as keys. */
function keys(data: CliData): Set<string> {
	const out = new Set<string>();
	for (const g of data.global) out.add(`opt:${GLOBAL}:${g.id}`);
	for (const c of data.commands) {
		out.add(`cmd:${c.path}`);
		for (const a of c.arguments) out.add(`arg:${c.path}:${a.id}`);
		for (const o of c.options) out.add(`opt:${c.path}:${o.id}`);
	}
	return out;
}
const releaseKeys = releases.map((r) => ({ version: r.version, keys: keys(r.data) }));

export type Mark = { unreleased: true } | { since: string } | null;

/** The version mark for `cmd:<path>`, `arg:<path>:<id>` or `opt:<path>:<id>`. */
export function mark(key: string, since?: string): Mark {
	const latest = releaseKeys.find((r) => r.version === latestRelease);
	if (!latest?.keys.has(key)) return { unreleased: true };
	if (since) return { since };
	const first = releaseKeys.find((r) => r.keys.has(key));
	if (!first || first.version === releaseKeys[0].version) return null;
	return { since: first.version };
}

export function getCommand(path: string): CliCommand {
	const c = cli.commands.find((x) => x.path === path);
	if (!c) throw new Error(`CLI reference: no command "${path}" in src/data/cli/main.json`);
	return c;
}

/** The page that documents a command: reference/cli/<page>/. */
export function pageOf(path: string): string | null {
	const [first, second] = path.split(' ');
	if (first === 'system') return second ? `system/${second}` : null;
	return first;
}

/**
 * The command a page is about: `system/keys` documents `system keys
 * generate`, its only subcommand; `config` documents `config`, with its
 * subcommands as sections (`#cmd-set`).
 */
export function pageCommand(page: string): string {
	const path = page.replace(/\//g, ' ');
	const c = cli.commands.find((x) => x.path === path);
	return c && c.subcommands.length === 1 ? c.subcommands[0] : path;
}

/** Where a command is documented: `/reference/cli/config/#cmd-set`. */
export function hrefOf(path: string): string | undefined {
	const page = pageOf(path);
	if (!page) return undefined;
	const own = pageCommand(page);
	const base = `/reference/cli/${page}/`;
	if (path === own || own.startsWith(`${path} `)) return base;
	return `${base}#cmd-${path.slice(own.length).trim().replace(/\s+/g, '-')}`;
}

/**
 * An entry's anchor. On the page of the command itself: `#arg-name`,
 * `#flag-unlock`; for a subcommand on the same page: `#arg-set-key`.
 */
export function anchor(path: string, kind: 'arg' | 'flag', id: string): string {
	const page = pageOf(path);
	const own = page ? pageCommand(page) : '';
	const rest = path === own ? '' : path.slice(own.length).trim();
	const sub = rest ? `${rest.replace(/\s+/g, '-')}-` : '';
	return `${kind}-${sub}${id}`;
}

/** The synopsis line, as `vorpal build <NAME> [OPTIONS]`. */
export function synopsis(c: CliCommand): string {
	const parts = [cli.name, c.path];
	for (const a of c.arguments) {
		if (a.trailing) continue;
		const v = `<${a.value_name ?? a.id.toUpperCase()}>`;
		parts.push(a.required ? v : `[${v}]`);
	}
	if (c.subcommands.length) parts.push('<COMMAND>');
	if (c.options.length) parts.push('[OPTIONS]');
	const trailing = c.arguments.find((a) => a.trailing);
	if (trailing) parts.push(`[-- <${trailing.value_name ?? trailing.id.toUpperCase()}>...]`);
	return parts.join(' ');
}

/** `--agent`, or `-h, --help`. */
export const flagName = (o: CliArg) => (o.long ? `--${o.long}` : o.short ? `-${o.short}` : o.id);

/** An option's key in overlay.yaml: its long name (`issuer-audience`). */
export const overlayKey = (o: CliArg) => o.long ?? o.id;

/** Checks overlay.yaml against main.json; throws on any gap either way. */
export function checkOverlay(overlay: Record<string, OverlayCommand>) {
	const problems: string[] = [];
	const commands: [string, CliArg[], CliArg[]][] = [
		[GLOBAL, [], cli.global],
		...cli.commands.map((c): [string, CliArg[], CliArg[]] => [c.path, c.arguments, c.options]),
	];
	for (const [path, args, opts] of commands) {
		const o = overlay[path];
		if (!o) {
			problems.push(`"${path}": no entry`);
			continue;
		}
		if (path !== GLOBAL && !o.summary) problems.push(`"${path}": no summary`);
		for (const [kind, list, given] of [
			['arguments', args, o.arguments ?? {}],
			['options', opts, o.options ?? {}],
		] as const) {
			const name = (a: CliArg) => (kind === 'options' ? overlayKey(a) : a.id);
			for (const a of list) if (!given[name(a)]?.text) problems.push(`"${path}" ${kind}.${name(a)}: no text`);
			for (const id of Object.keys(given))
				if (!list.some((a) => name(a) === id)) problems.push(`"${path}" ${kind}.${id}: not in the CLI`);
		}
	}
	for (const path of Object.keys(overlay))
		if (!commands.some(([p]) => p === path)) problems.push(`"${path}": not a command in the CLI`);
	if (problems.length) {
		throw new Error(
			`src/data/cli/overlay.yaml and main.json differ (add text for new flags, remove old ones):\n  ${problems.join('\n  ')}`,
		);
	}
}

/** How an entry shows its default, as inline Markdown. */
export function defaultText(o: CliArg, given?: string): string {
	if (given) return given;
	if (o.action === 'flag') return o.default === 'true' ? 'on' : 'off';
	const d = o.default;
	if (d === null || (Array.isArray(d) && d.length === 0)) return 'not set';
	const list = Array.isArray(d) ? d : [d];
	return list
		.map((v) => (cli.placeholders[v] ? cli.placeholders[v] : `\`${v}\``))
		.join(', ');
}
