import { defineCollection, reference } from 'astro:content';
import { file, glob } from 'astro/loaders';
import { z } from 'astro/zod';
import { docsLoader } from '@astrojs/starlight/loaders';
import { docsSchema } from '@astrojs/starlight/schema';

/**
 * Page frontmatter for the redesign (docs-redesign stage 4, decisions 7, 14
 * and the page header). Every field is optional, so existing pages validate
 * unchanged; the page shell reads them.
 *
 * - `kind`: the page type. It picks the page header's badge and which
 *   headings the sectionize plugin wraps (bench steps, causes, entries).
 * - `summary`: the one-sentence summary under the h1.
 * - `since`: the release that introduced what the page documents ("0.6.0").
 * - `unreleased`: true while what the page documents is on main only.
 * - `wide`: use the 72rem column (tutorials, how-tos, signature concepts).
 * - `lastUpdated`: shown in the page footer. Starlight's own `lastUpdated`
 *   already takes a date or a boolean, so this extends it rather than adding
 *   a second field.
 * - `related`: pages for the Related cards, by collection id
 *   (`concepts/artifacts`).
 * - `metaNote`: a short note after the versions in the page header's meta
 *   line ("Generated from the CLI source").
 */
const redesign = z.object({
	kind: z
		// The templates' kind badges, in order: Tutorial, How-to, Operator
		// how-to, Concept, Reference · CLI / Config / SDK / Errors,
		// Troubleshooting, SDK hub; plus the home page and the 404 page.
		.enum([
			'tutorial',
			'how-to',
			'operator-how-to',
			'concept',
			'reference-cli',
			'reference-config',
			'reference-sdk',
			'reference-errors',
			'troubleshooting',
			'sdk-hub',
			'home',
			'not-found',
		])
		.optional(),
	summary: z.string().optional(),
	since: z
		.string()
		.regex(/^\d+\.\d+\.\d+$/, 'since: a release version such as 0.6.0')
		.optional(),
	unreleased: z.boolean().optional(),
	wide: z.boolean().optional(),
	lastUpdated: z.union([z.coerce.date(), z.boolean()]).optional(),
	related: z.array(reference('docs')).optional(),
	metaNote: z.string().optional(),
});

/** One argument or flag in src/data/cli/overlay.yaml (see the file's header). */
const overlayItem = z
	.object({
		text: z.string(),
		value_name: z.string().optional(),
		default: z.string().optional(),
		setting: z.string().optional(),
		env: z.string().optional(),
		why: z.string().optional(),
		since: z.string().regex(/^\d+\.\d+\.\d+$/).optional(),
	})
	.strict();

/**
 * The hand-written text of the generated CLI reference: one entry per
 * command path ("build", "system services start"), plus `_global`.
 */
const overlayCommand = z
	.object({
		summary: z.string().optional(),
		arguments: z.record(z.string(), overlayItem).optional(),
		options: z.record(z.string(), overlayItem).optional(),
	})
	.strict();
const cliOverlay = defineCollection({
	loader: file('src/data/cli/overlay.yaml'),
	schema: overlayCommand,
});

/**
 * Version snapshots keep their own copy of the data their pages render
 * (src/versions.mjs). Ids keep the dots of the version: `0.6` and
 * `0.6/source-changed`.
 */
const snapshotId = ({ entry }: { entry: string }) => entry.replace(/\.yaml$/, '').replace(/\/(errors|cli-overlay)(?=\/|$)/, '');

/** src/snapshots/<x.y>/cli-overlay.yaml: one entry per snapshot, id `<x.y>`. */
const snapshotCliOverlay = defineCollection({
	loader: glob({ pattern: '*/cli-overlay.yaml', base: './src/snapshots', generateId: snapshotId }),
	schema: z.record(z.string(), overlayCommand),
});

/** Where a message is raised: CI checks that `match` is still in `file`. */
const errorSource = z
	.object({
		file: z.string(),
		line: z.number().int().positive(),
		match: z.string(),
	})
	.strict();

/** A step of an error's fix: prose (inline Markdown), or a command and its output. */
const errorStep = z.union([
	z.object({ text: z.string() }).strict(),
	z.object({ command: z.string(), output: z.string().optional(), label: z.string().optional() }).strict(),
]);

/**
 * The Error reference (src/content/errors/<id>.yaml, one message per file;
 * the file name is the id and the page anchor is `#err-<id>`). Text fields
 * are inline Markdown. Messages are word for word, with `<placeholders>`.
 * script/website-checks/reference.mjs checks each `sources[].match` against
 * the Rust and SDK sources.
 */
const errorSchema = z
		.object({
			group: z.enum(['lockfile', 'config', 'settings', 'connection']),
			order: z.number().int(),
			message: z.string(),
			variants: z.array(z.object({ who: z.string(), message: z.string() }).strict()).optional(),
			raised_by: z.string(),
			meaning: z.string(),
			fix: z.array(errorStep).min(1),
			check: z.string(),
			links: z.array(z.object({ text: z.string(), href: z.string() }).strict()).default([]),
			sources: z.array(errorSource).min(1),
		})
		.strict();
const errors = defineCollection({
	loader: glob({ pattern: '*.yaml', base: './src/content/errors' }),
	schema: errorSchema,
});

/** src/snapshots/<x.y>/errors/<id>.yaml, id `<x.y>/<id>`. Not checked against the sources. */
const snapshotErrors = defineCollection({
	loader: glob({ pattern: '*/errors/*.yaml', base: './src/snapshots', generateId: snapshotId }),
	schema: errorSchema,
});

export const collections = {
	docs: defineCollection({
		loader: docsLoader(),
		schema: docsSchema({ extend: redesign }),
	}),
	cliOverlay,
	errors,
	snapshotCliOverlay,
	snapshotErrors,
};
