fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure().compile(
        &["config/config.proto", "package/package.proto"],
        &["api/v0"],
    )?;
    Ok(())
}
