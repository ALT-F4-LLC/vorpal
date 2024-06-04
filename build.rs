fn main() -> Result<(), Box<dyn std::error::Error>> {
    tonic_build::compile_protos("api/v0/cli/cli.proto")?;
    tonic_build::compile_protos("api/v0/package/package.proto")?;
    Ok(())
}
