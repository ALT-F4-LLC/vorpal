import {
    ConfigContext,
    TypeScript,
    TypeScriptDevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

const ctx = ConfigContext.create();

const systems = [
    "aarch64-darwin",
    "aarch64-linux",
    "x86_64-darwin",
    "x86_64-linux",
];

await new TypeScriptDevelopmentEnvironment("example-shell", systems)
    .build(ctx);

await new TypeScript("example", systems)
    .withEntrypoint("src/main.ts")
    .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
    .build(ctx);

await ctx.run();
