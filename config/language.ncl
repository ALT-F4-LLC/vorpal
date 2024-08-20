let { Package, .. } = import "schema.ncl" in
{
  RustPackage = fun config =>
    let protocSystem = fun label value =>
      value
      |> match {
        "aarch64-linux" => "linux-aarch_64",
        "x86_64-linux" => "linux-x86_64",
        _ => std.fail_with "Unsupported target"
      }
    in
    let protocSystem = "%{config.target | protocSystem}" in
    let protocVersion = "25.4" in
    let rustBaseUrl = "https://static.rust-lang.org/dist/2024-05-02" in
    let rustSystem = fun label value =>
      value
      |> match {
        "aarch64-linux" => "aarch64-unknown-linux-gnu",
        "x86_64-linux" => "x86_64-unknown-linux-gnu",
        _ => std.fail_with "Unsupported target"
      }
    in
    let rustSystem = "%{config.target | rustSystem}" in
    let rustVersion = "1.78.0" in

    let cargo =
      {
        name = "cargo",
        script = m%"
          cp -pr cargo-%{rustVersion}-%{rustSystem}/cargo/* $output/.
        "%,
        source = "%{rustBaseUrl}/cargo-%{rustVersion}-%{rustSystem}.tar.gz",
        source_hash = "d782e34151df01519de86f0acace8a755cae6fad93cb0303ddd61c2642444c1c",
        systems = config.systems,
        target = config.target,
      } | Package
    in

    let rust_std =
      {
        name = "rust-std",
        script = m%"
          cp -pr rust-std-%{rustVersion}-%{rustSystem}/rust-std-%{rustSystem}/* $output/.
        "%,
        source = "%{rustBaseUrl}/rust-std-%{rustVersion}-%{rustSystem}.tar.gz",
        source_hash = "72d4917bb58b693b3f2c589746ed470645f96895ece3dd27f7055d3c3f7f7a79",
        systems = config.systems,
        target = config.target,
      } | Package
    in

    let rustc =
      {
        name = "rustc",
        packages = [rust_std],
        script = m%"
          cp -pr rustc-%{rustVersion}-%{rustSystem}/rustc/* $output/.
          cat $rust_std/manifest.in >> $output/manifest.in
          cp -pr $rust_std/lib $output
        "%,
        source = "%{rustBaseUrl}/rustc-%{rustVersion}-%{rustSystem}.tar.gz",
        source_hash = "bc6c0e0f309805c4a9b704bbfe6be6b3c28b029ac6958c58ab5b90437a9e36ed",
        systems = config.systems,
        target = config.target,
      } | Package
    in

    let protoc =
      {
        name = "protoc",
        script = m%"
          cp -pr ./bin ./include $output/.
          chmod +x $output/bin/protoc
        "%,
        source = "https://github.com/protocolbuffers/protobuf/releases/download/v%{protocVersion}/protoc-%{protocVersion}-%{protocSystem}.zip",
        source_hash = "8a592a0dd590e92b1c0d77631e683fc743d1ed8158e0b093b6cfabf0685089af",
        systems = config.systems,
        target = config.target,
      } | Package
    in

    let sourceIgnores = [".buildx", ".direnv", ".git", ".gitignore", "target", "vendor"] in

    let vendor =
      {
        name = "%{config.name}-vendor",
        packages = [cargo, rustc],
        script = m%"
          mkdir -p cli/src \
            && mkdir -p notary/src \
            && mkdir -p schema/src \
            && mkdir -p store/src \
            && mkdir -p worker/src \
            && mkdir -p $output/vendor \

          touch cli/src/main.rs \
            && touch notary/src/lib.rs \
            && touch schema/src/lib.rs \
            && touch store/src/lib.rs \
            && touch worker/src/main.rs

          export CARGO_VENDOR=$(cargo vendor --versioned-dirs $output/vendor)

          echo "$CARGO_VENDOR" > $output/config.toml
        "%,
        source_ignores = std.array.concat sourceIgnores ["cli/src", "config", "notary/src", "schema/api", "schema/src", "schema/build.rs", "store/src", "worker/src", ".dockerignore", ".envrc", ".github", "Dockerfile", "Dockerfile.sandbox", "LICENSE", "README.md", "flake.lock", "flake.nix", "justfile", "vorpal.webp"],
        source = config.source,
        systems = config.systems,
        target = config.target,
      } | Package
    in

    {
      name = config.name,
      packages = [cargo, rustc, protoc, vendor],
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
      target = config.target,
    } | Package
}
