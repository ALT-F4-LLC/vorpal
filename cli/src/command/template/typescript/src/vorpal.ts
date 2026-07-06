import {
    ArtifactSystem,
    ConfigContext,
    TypeScript,
    TypeScriptDevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

const ctx = ConfigContext.create();

const systems: ArtifactSystem[] = [
    ArtifactSystem.AARCH64_DARWIN,
    ArtifactSystem.AARCH64_LINUX,
    ArtifactSystem.X8664_DARWIN,
    ArtifactSystem.X8664_LINUX,
];

await new TypeScriptDevelopmentEnvironment("example-shell", systems)
    .build(ctx);

await new TypeScript("example", systems)
    .withEntrypoint("src/main.ts")
    .withIncludes(["src", "bun.lock", "package.json", "tsconfig.json"])
    .build(ctx);

await ctx.run();
