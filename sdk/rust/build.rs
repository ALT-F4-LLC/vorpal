fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_prost_build::configure()
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
            "vorpal.artifact.ArtifactFunctionParam",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.artifact.ArtifactFunction",
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
                "context/context.proto",
                "worker/worker.proto",
            ],
            &["api"],
        )?;
    Ok(())
}
