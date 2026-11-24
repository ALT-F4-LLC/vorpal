import {
    ConfigContext,
    SYSTEMS,
    TypeScript,
    TypeScriptDevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

const ctx = ConfigContext.create();

// -> 1. Activate: `source "$(vorpal build --path 'hello-world-dev')/bin/activate"`
// -> 2. Deactivate: `deactivate`

await new TypeScriptDevelopmentEnvironment("hello-world-dev", SYSTEMS)
    .build(ctx);

// -> 1. Build: `vorpal build 'hello-world'`
// -> 2. Run: `$(vorpal build --path 'hello-world')/bin/hello-world`

await new TypeScript("hello-world", SYSTEMS)
    .withEntrypoint("src/main.ts")
    .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
    .build(ctx);

await ctx.run();
