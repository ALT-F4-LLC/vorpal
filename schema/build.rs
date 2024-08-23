fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .enum_attribute(
            "vorpal.package.v0.PackageSystem",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .message_attribute(
            "vorpal.package.v0.PackageInput",
            "#[derive(serde::Serialize, serde::Deserialize)]",
        )
        .compile(&["package/package.proto", "store/store.proto"], &["api/v0"])?;
    Ok(())
}
