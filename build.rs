fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::configure()
        .compile(&["build/build.proto", "package/package.proto"], &["api/v0"])?;
    Ok(())
}
