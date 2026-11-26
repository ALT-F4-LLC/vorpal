script := fmt.Sprintf("%s/bin/my-tool --version", artifact.GetEnvKey(*tool))

artifact.NewJob("my-job", script, config.SYSTEMS).
    WithArtifacts([]*string{tool}).
    Build(ctx)
