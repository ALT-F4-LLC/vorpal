// @ts-check
/**
 * The sidebar of the docs for main (Starlight's `sidebar` config). Also
 * read by the Markdown copies for llms.txt's sections, and frozen into each
 * version snapshot (src/snapshots/<x.y>/snapshot.json) by
 * script/website-versions/snapshot.mjs.
 *
 * Slugs only: a version snapshot prefixes them with `v/<x.y>/`.
 * @type {import('./integrations/markdown-twins.mjs').SidebarItem[]}
 */
export const sidebar = [
	{
		// The header's section names match these group labels (src/nav.ts).
		label: 'Get started',
		items: [
			{ label: 'Installation', slug: 'getting-started/installation' },
			{ label: 'Quickstart', slug: 'getting-started/quickstart' },
		],
	},
	{
		label: 'Guides',
		items: [
			{ label: 'Add a development environment', slug: 'guides/development-environment' },
			{
				// Every task in one SDK, on one page each. They become the
				// generated per-SDK views (ia-proposal, Question 2 B).
				label: 'Every guide in one SDK',
				items: [
					{ label: 'Python', slug: 'guides/python' },
					{ label: 'TypeScript', slug: 'guides/typescript' },
					{ label: 'Go', slug: 'guides/go' },
					{ label: 'Rust', slug: 'guides/rust' },
				],
			},
		],
	},
	{
		label: 'Troubleshooting',
		items: [{ label: 'Error reference', slug: 'troubleshooting/errors' }],
	},
	{
		label: 'Concepts',
		items: [
			{ label: 'Architecture', slug: 'concepts/architecture' },
			{ label: 'How a build runs', slug: 'concepts/how-a-build-runs' },
			{ label: 'Artifacts', slug: 'concepts/artifacts' },
			{ label: 'Caching', slug: 'concepts/caching' },
			{ label: 'Environments', slug: 'concepts/environments' },
			{ label: 'Jobs', slug: 'concepts/jobs' },
			{ label: 'Processes', slug: 'concepts/processes' },
			{ label: 'How Vorpal compares', slug: 'concepts/how-vorpal-compares' },
		],
	},
	{
		label: 'Self-hosting',
		items: [{ label: 'Set up TLS and client trust', slug: 'self-hosting/tls' }],
	},
	{
		// Not a header section: the SDK hubs sit beside the sections
		// (src/nav.ts, EXTRA_SECTIONS). Site order: Python first.
		label: 'SDKs',
		items: [
			{ label: 'Python SDK', slug: 'sdks/python' },
			{ label: 'TypeScript SDK', slug: 'sdks/typescript' },
			{ label: 'Go SDK', slug: 'sdks/go' },
			{ label: 'Rust SDK', slug: 'sdks/rust' },
		],
	},
	{
		label: 'Reference',
		items: [
			{
				label: 'CLI',
				items: [
					{ label: 'Overview', slug: 'reference/cli' },
					{ label: 'vorpal build', slug: 'reference/cli/build' },
					{ label: 'vorpal prepare', slug: 'reference/cli/prepare' },
					{ label: 'vorpal run', slug: 'reference/cli/run' },
					{ label: 'vorpal inspect', slug: 'reference/cli/inspect' },
					{ label: 'vorpal init', slug: 'reference/cli/init' },
					{ label: 'vorpal config', slug: 'reference/cli/config' },
					{ label: 'vorpal login', slug: 'reference/cli/login' },
					{ label: 'vorpal system keys', slug: 'reference/cli/system/keys' },
					{ label: 'vorpal system prune', slug: 'reference/cli/system/prune' },
					{ label: 'vorpal system services', slug: 'reference/cli/system/services' },
				],
			},
			{ label: 'Configuration', slug: 'reference/configuration' },
			{ label: 'API', slug: 'reference/api' },
			{ label: 'Contributing to Docs', slug: 'reference/contributing' },
		],
	},
];
