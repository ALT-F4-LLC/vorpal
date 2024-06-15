fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure().compile(
        &["command/command.proto", "package/package.proto"],
        &["api/v0"],
    )?;
    Ok(())
}
