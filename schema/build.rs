fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .message_attribute(
            "vorpal.config.v0.Config",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.package.v0.PackageOutput",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.package.v0.PackageSource",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.package.v0.Package",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .compile_protos(
            &[
                "v0/config/config.proto",
                "v0/package/package.proto",
                "v0/store/store.proto",
                "v0/worker/worker.proto",
            ],
            &["api"],
        )?;
    Ok(())
}
