from vorpal_sdk import SYSTEMS, ConfigContext, Protoc, PythonDevelopmentEnvironment

ctx = ConfigContext.create()

protoc = Protoc().build(ctx)

(
    PythonDevelopmentEnvironment("my-project-dev", SYSTEMS)
    .with_artifacts([protoc])
    .with_environments(["PYTHONWARNINGS=default"])
    .build(ctx)
)

ctx.run()
