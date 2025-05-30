// @ts-check
import { defineConfig } from 'astro/config';
import starlight from '@astrojs/starlight';

// https://astro.build/config
export default defineConfig({
  site: 'https://alt-f4-llc.github.io',
  base: 'vorpal',
	integrations: [
		starlight({
			title: 'Vorpal',
      editLink: {
        baseUrl: 'https://github.com/withastro/starlight/edit/main/docs/',
      },
			social: [{ icon: 'github', label: 'GitHub', href: 'https://github.com/ALT-F4-LLC/vorpal' }],
			sidebar: [
				{
					label: 'Guides',
					autogenerate: { directory: 'guides' },
				},
				{
					label: 'Reference',
					autogenerate: { directory: 'reference' },
				},
			],
		}),
	],
});
