// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
	integrations: [
		starlight({
			title: 'Vorpal',
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/ALT-F4-LLC/vorpal' }],
			sidebar: [
				{
					label: 'Guides',
					autogenerate: { directory: 'reference' },
				},
				{
					label: 'Reference',
					autogenerate: { directory: 'reference' },
				},
			],
		}),
	],
});
