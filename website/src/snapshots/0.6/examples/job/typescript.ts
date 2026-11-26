import {
  ConfigContext,
  Job,
  SYSTEMS,
  getEnvKey,
} from "@altf4llc/vorpal-sdk";

const context = ConfigContext.create();

const tool = await buildMyTool(context);
const script = `${getEnvKey(tool)}/bin/my-tool --version`;

await new Job("my-job", script, SYSTEMS)
  .withArtifacts([tool])
  .build(context);

await context.run();
