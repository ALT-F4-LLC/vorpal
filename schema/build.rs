fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .enum_attribute(
            "vorpal.package.v0.PackageSystem",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.config.v0.Config",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.package.v0.PackageOutput",
            "#[derive(Eq, Hash, PartialOrd, Ord, serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.package.v0.PackageEnvironment",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.package.v0.PackageSandbox",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.package.v0.PackageSandboxPath",
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
