import {
  ConfigContext,
  Protoc,
  SYSTEMS,
  TypeScriptDevelopmentEnvironment,
} from "@altf4llc/vorpal-sdk";

const ctx = ConfigContext.create();

const protoc = await new Protoc().build(ctx);

await new TypeScriptDevelopmentEnvironment("my-project-dev", SYSTEMS)
  .withArtifacts([protoc])
  .withEnvironments(["NODE_ENV=development"])
  .build(ctx);

await ctx.run();
