from vorpal_sdk import SYSTEMS, ConfigContext, Job, get_env_key

ctx = ConfigContext.create()

tool = build_my_tool(ctx)
script = f"{get_env_key(tool)}/bin/my-tool --version"

Job("my-job", script, SYSTEMS).with_artifacts([tool]).build(ctx)

ctx.run()
