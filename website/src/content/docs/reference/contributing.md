---
title: Contributing to Docs
description: How to add and edit pages on the Vorpal documentation site.
---

The Vorpal documentation site is built with [Starlight](https://starlight.astro.build/) (Astro) and lives in the `website/` directory of the repository. This guide covers everything you need to contribute.

## Prerequisites

You need [Bun](https://bun.sh/) installed. Node.js works too, but the CI pipeline uses Bun.

## Local development

Clone the repository and start the dev server:

```bash
git clone https://github.com/ALT-F4-LLC/vorpal.git
cd vorpal/website
bun install
bun run dev
```

The dev server starts at `http://localhost:4321` with hot reload -- edits to markdown files appear instantly in the browser.

To run a production build locally (useful for catching broken links or build errors):

```bash
bun run build
```

## Content structure

Documentation pages live under `website/src/content/docs/` organized by section:

```
src/content/docs/
  index.md                          # Landing page
  getting-started/
    installation.md                 # Install guide
    quickstart.md                   # First project walkthrough
  guides/
    rust.md                         # Rust SDK guide
    go.md                           # Go SDK guide
    typescript.md                   # TypeScript SDK guide
  concepts/
    architecture.md                 # System design
    artifacts.md                    # Artifact model
    caching.md                      # Caching strategy
    environments.md                 # Dev/user environments
  reference/
    cli.md                          # CLI command reference
    configuration.md                # Config file reference
    api.md                          # API reference
    contributing.md                 # This page
```

## Adding a page

1. Create a markdown file in the appropriate section directory (e.g., `concepts/new-topic.md`).

2. Add frontmatter at the top:

   ```markdown
   ---
   title: Page Title
   description: A one-line summary shown in search results and metadata.
   ---

   Your content here.
   ```

3. Add the page to the sidebar in `website/astro.config.mjs`:

   ```javascript
   {
     label: 'Concepts',
     items: [
       // existing items...
       { label: 'New Topic', slug: 'concepts/new-topic' },
     ],
   },
   ```

## Writing content

### Markdown basics

Pages use standard markdown. Starlight adds syntax highlighting for code blocks automatically -- just specify the language:

````markdown
```rust
fn main() {
    println!("Hello, world!");
}
```
````

You can add a title to code blocks with the `title` attribute:

````markdown
```toml title="Vorpal.toml"
[config]
language = "rust"
```
````

### Tabbed code examples

For pages that show the same example in multiple languages, use `.mdx` (not `.md`) and import the Starlight Tabs component:

```mdx
---
title: My Guide
description: A guide with multi-language examples.
---

import { Tabs, TabItem } from '@astrojs/starlight/components';

<Tabs>
  <TabItem label="TypeScript">
    ```typescript
    const context = ConfigContext.create();
    ```
  </TabItem>
  <TabItem label="Rust">
    ```rust
    let ctx = &mut get_context().await?;
    ```
  </TabItem>
  <TabItem label="Go">
    ```go
    ctx := config.GetContext()
    ```
  </TabItem>
</Tabs>
```

If a page needs tabbed examples, use the `.mdx` extension for that page. The current SDK guide pages (`guides/*.md`) use separate code blocks per language instead of tabs.

### Links

Use relative paths for links between documentation pages:

```markdown
See the [Quickstart](../getting-started/quickstart) for a walkthrough.
```

Do not use absolute paths (like `/getting-started/quickstart`) -- relative paths ensure links work regardless of the deployment domain.

## PR preview deployments

When you open a pull request that changes files in `website/`, the CI pipeline automatically:

1. Builds the site
2. Runs link validation
3. Deploys a preview to Cloudflare Pages

The preview URL appears in the PR checks. Reviewers can use it to see your rendered changes without building locally.

## Further reading

For advanced Starlight features (component overrides, custom CSS, Expressive Code options), see the [Starlight documentation](https://starlight.astro.build/).
