import { defineCollection, reference } from 'astro:content';
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
 */
const redesign = z.object({
	kind: z
		// The templates' kind badges, in order: Tutorial, How-to, Operator
		// how-to, Concept, Reference · CLI / Config / SDK / Errors,
		// Troubleshooting, SDK hub; plus the home page.
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
});

export const collections = {
	docs: defineCollection({
		loader: docsLoader(),
		schema: docsSchema({ extend: redesign }),
	}),
};
