fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .message_attribute(
            "vorpal.artifact.v0.ArtifactSource",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.artifact.v0.ArtifactStep",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.artifact.v0.Artifact",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.artifact.v0.Artifacts",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .compile_protos(
            &[
                "v0/agent/agent.proto",
                "v0/archive/archive.proto",
                "v0/artifact/artifact.proto",
                "v0/worker/worker.proto",
            ],
            &["api"],
        )?;
    Ok(())
}
