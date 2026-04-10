import {
    ArtifactSystem,
    ConfigContext,
    TypeScript,
    TypeScriptDevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

// Define build context

const ctx = ConfigContext.create();

// Define supported artifact systems

const systems: ArtifactSystem[] = [
    ArtifactSystem.AARCH64_DARWIN,
    ArtifactSystem.AARCH64_LINUX,
    ArtifactSystem.X8664_DARWIN,
    ArtifactSystem.X8664_LINUX,
];

// Define language-specific development environment artifact

await new TypeScriptDevelopmentEnvironment("example-shell", systems)
    .build(ctx);

// Define application artifact 

await new TypeScript("example", systems)
    .withEntrypoint("src/main.ts")
    .withIncludes(["src", "package.json", "tsconfig.json", "bun.lock"])
    .build(ctx);

// Run context to build

await ctx.run();
