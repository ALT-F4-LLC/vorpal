import {
    ConfigContext,
    SYSTEMS,
    TypeScript,
    TypeScriptDevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

const ctx = ConfigContext.create();

// -> 1. Activate: `source "$(vorpal build --path 'example-dev')/bin/activate"`
// -> 2. Deactivate: `deactivate`

await new TypeScriptDevelopmentEnvironment("example-dev", SYSTEMS)
    .build(ctx);

// -> 1. Build: `vorpal build 'example'`
// -> 2. Run: `$(vorpal build --path 'example')/bin/example`

await new TypeScript("example", SYSTEMS)
    .withEntrypoint("src/main.ts")
    .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
    .build(ctx);

await ctx.run();
