fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .message_attribute(
            "vorpal.artifact.ArtifactSource",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.artifact.ArtifactStepSecret",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.artifact.ArtifactStep",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.artifact.Artifact",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.artifact.Artifacts",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .compile_protos(
            &[
                "agent/agent.proto",
                "archive/archive.proto",
                "artifact/artifact.proto",
                "worker/worker.proto",
            ],
            &["api"],
        )?;
    Ok(())
}
