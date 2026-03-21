// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	site: 'https://docs.vorpal.build',
	integrations: [
		starlight({
			title: 'Vorpal',
			social: [
				{
					icon: 'github',
					label: 'GitHub',
					href: 'https://github.com/ALT-F4-LLC/vorpal',
				},
			],
			customCss: ['./src/styles/custom.css'],
			sidebar: [
				{
					label: 'Getting Started',
					items: [
						{ label: 'Installation', slug: 'getting-started/installation' },
						{ label: 'Quickstart', slug: 'getting-started/quickstart' },
					],
				},
				{
					label: 'Guides',
					items: [
						{ label: 'Go', slug: 'guides/go' },
						{ label: 'Rust', slug: 'guides/rust' },
						{ label: 'TypeScript', slug: 'guides/typescript' },
					],
				},
				{
					label: 'Concepts',
					items: [
						{ label: 'Architecture', slug: 'concepts/architecture' },
						{ label: 'Artifacts', slug: 'concepts/artifacts' },
						{ label: 'Caching', slug: 'concepts/caching' },
						{ label: 'Environments', slug: 'concepts/environments' },
						{ label: 'Jobs', slug: 'concepts/jobs' },
						{ label: 'Processes', slug: 'concepts/processes' },
					],
				},
				{
					label: 'Reference',
					items: [
						{ label: 'Command-line (CLI)', slug: 'reference/cli' },
						{ label: 'Configuration', slug: 'reference/configuration' },
						{ label: 'API', slug: 'reference/api' },
						{ label: 'Contributing to Docs', slug: 'reference/contributing' },
					],
				},
			],
		}),
	],
});
