let { Package, .. } = import "@vorpal/schema.ncl" in
{
  RustPackage = fun config =>
    let protocBaseUrl = "https://github.com/protocolbuffers/protobuf/releases/download" in
    let protocSystem = fun label value =>
      value
      |> match {
        "aarch64-linux" => "linux-aarch_64",
        "aarch64-macos" => "osx-aarch_64",
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
        "aarch64-macos" => "aarch64-apple-darwin",
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
        source_hash =
          config.target
          |> match {
            "aarch64-linux" => "d782e34151df01519de86f0acace8a755cae6fad93cb0303ddd61c2642444c1c",
            "aarch64-macos" => "d8ed8e9f5ceefcfe3bca7acd0797ade24eadb17ddccaa319cd00ea290f598d00",
            "x86_64-linux" => "d8ed8e9f5ceefcfe3bca7acd0797ade24eadb17ddccaa319cd00ea290f598d00",
            _ => std.fail_with "Unsupported target"
          },
        systems = config.systems,
      } | Package
    in

    let rust_std =
      {
        name = "rust-std",
        script = m%"
          cp -pr rust-std-%{rustVersion}-%{rustSystem}/rust-std-%{rustSystem}/* $output/.
        "%,
        source = "%{rustBaseUrl}/rust-std-%{rustVersion}-%{rustSystem}.tar.gz",
        source_hash =
          config.target
          |> match {
            "aarch64-linux" => "72d4917bb58b693b3f2c589746ed470645f96895ece3dd27f7055d3c3f7f7a79",
            "aarch64-macos" => "0689a9b2dec87c272954db9212a8f3d5243f55f777f90d84d2b3aeb2aa938ba5",
            "x86_64-linux" => "ad734eb9699b0a9dffdd35034776ccaa4d7b45e1898fc32748be93b60453550d",
            _ => std.fail_with "Unsupported target"
          },
        systems = config.systems,
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
        source_hash =
          config.target
          |> match {
            "aarch64-linux" => "bc6c0e0f309805c4a9b704bbfe6be6b3c28b029ac6958c58ab5b90437a9e36ed",
            "aarch64-macos" => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
            "x86_64-linux" => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
            _ => std.fail_with "Unsupported target"
          },
        systems = config.systems,
      } | Package
    in

    let protoc =
      {
        name = "protoc",
        script = m%"
          cp -pr ./bin ./include $output/.
          chmod +x $output/bin/protoc
        "%,
        source = "%{protocBaseUrl}/v%{protocVersion}/protoc-%{protocVersion}-%{protocSystem}.zip",
        source_hash =
          config.target
          |> match {
            "aarch64-linux" => "8a592a0dd590e92b1c0d77631e683fc743d1ed8158e0b093b6cfabf0685089af",
            "aarch64-macos" => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
            "x86_64-linux" => "d105abb1c1d2c024f29df884f0592f1307984d63aeb10f0e61ccb94aee2c2feb",
            _ => std.fail_with "Unsupported target"
          },
        systems = config.systems,
      } | Package
    in

    let sourceExcludes = [
      ".dockerignore",
      ".envrc",
      ".git",
      ".github",
      ".gitignore",
      ".vorpal",
      "Dockerfile",
      "Dockerfile.sandbox",
      "LICENSE",
      "README.md",
      "deps",
      "dev.sh",
      "justfile",
      "nickel",
      "script",
      "target",
      "vorpal.lock.ncl",
      "vorpal.ncl",
      "vorpal.png",
    ]
    in

    let sourceIncludes = [
      "Cargo.lock",
      "Cargo.toml",
      "cli/Cargo.toml",
      "notary/Cargo.toml",
      "schema/Cargo.toml",
      "store/Cargo.toml",
      "worker/Cargo.toml"
    ]
    in

    let cache =
      {
        name = "%{config.name}-cache",
        packages = [cargo, rustc],
        script = m%"
          dirs=("cli/src" "notary/src" "schema/src" "store/src" "worker/src")

          for dir in "${dirs[@]}"; do
              mkdir -p "$dir"
          done

          for dir in "${dirs[@]}"; do
              if [[ "$dir" == "cli/src" ]]; then
                  touch "$dir/main.rs"
              else
                  touch "$dir/lib.rs"
              fi
          done

          mkdir -p $output/vendor

          export CARGO_VENDOR=$(cargo vendor --versioned-dirs $output/vendor)

          echo "$CARGO_VENDOR" > $output/config.toml
        "%,
        source = config.source,
        source_hash = config.cargo_hash,
        source_includes = sourceIncludes,
        systems = config.systems,
      } | Package
    in

    {
      name = config.name,
      packages = [cargo, rustc, protoc, cache],
      script = m%"
          mkdir -p .cargo

          cp $%{std.string.replace "-" "_" config.name}_cache/config.toml \
            .cargo/config.toml

          cargo build --offline --release

          cargo test --offline --release

          mkdir -p $output/bin

          cp -pr target/release/%{config.name} $output/bin/%{config.name}
        "%,
      source_excludes = sourceExcludes,
      source = config.source,
      systems = config.systems,
    } | Package
}
