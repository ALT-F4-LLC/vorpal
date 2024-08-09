let { Package, .. } = import "schema.ncl" in
{
  PackageRust = fun config =>
    let releaseBaseUrl = "https://static.rust-lang.org/dist/2024-05-02" in

    let releaseSystem = fun label value =>
      value
      |> match {
        "X8664Linux" => "x86_64-unknown-linux-gnu",
        _ => std.fail_with "Unsupported target"
      }
    in

    let releaseVersion = "1.78.0" in

    let releaseSystem = "%{config.target | releaseSystem}" in

    let cargo =
      {
        input.release = "%{releaseBaseUrl}/cargo-%{releaseVersion}-%{releaseSystem}.tar.gz",
        name = "cargo",
        script = "cp -pr $release/cargo/* $output/.",
        systems = config.systems,
      } | Package
    in

    let rust_std =
      {
        input.release = "%{releaseBaseUrl}/rust-std-%{releaseVersion}-%{releaseSystem}.tar.gz",
        name = "rust-std",
        script = "cp -pr $release/* $output/.",
        systems = config.systems,
      } | Package
    in

    let rustc =
      {
        input.release = "%{releaseBaseUrl}/rustc-%{releaseVersion}-%{releaseSystem}.tar.gz",
        name = "rustc",
        packages = [rust_std],
        script = m%"
          cp -pr $release/rustc/* $output/.
          cat $rust_std/manifest.in >> $output/manifest.in
          cp -pr $rust_std/lib $output
        "%,
        systems = config.systems,
      } | Package
    in

    let sourceIgnores = [".git", ".gitignore", "target", "vendor"] in

    let vendor =
      {
        name = "%{config.name}-vendor",
        packages = [cargo, rustc],
        script = m%"
          mkdir -p $output/vendor
          export CARGO_VENDOR=$(cargo vendor --versioned-dirs $output/vendor)
          echo "$CARGO_VENDOR" > $output/config.toml
        "%,
        source_ignores = sourceIgnores,
        source = config.source,
        systems = config.systems,
      } | Package
    in

    {
      name = config.name,
      packages = [cargo, rustc, vendor],
      script = m%"
          mkdir -p .cargo
          ln -s $%{std.string.replace "-" "_" config.name}_vendor/config.toml .cargo/config.toml
          cargo check -j $(nproc) --offline --profile release
          cargo build -j $(nproc) --offline --profile release
          cargo test -j $(nproc) --offline --profile release -- --test-threads=$(nproc)
          mkdir -p $output/bin
          cp -pr target/release/%{config.name} $output/bin/%{config.name}
        "%,
      source_ignores = sourceIgnores,
      source = config.source,
      systems = config.systems,
    } | Package
}
