let { Package, .. } = import "@vorpal/contract.ncl" in

let build_cargo = import "@vorpal/package/cargo.ncl" in
let build_protoc = import "@vorpal/package/protoc.ncl" in
let build_rustc = import "@vorpal/package/rustc.ncl" in
let build_sandbox = import "@vorpal/package/sandbox.ncl" in

fun config =>
  let cargo = build_cargo config.target in
  let rustc = build_rustc config.target in
  let protoc = build_protoc config.target in

  let cache =
    {
      name = "%{config.name}-cache",
      packages = [cargo, rustc],
      sandbox = build_sandbox config.target,
      script = m%"
        dirs=("cli/src" "notary/src" "schema/src" "store/src" "worker/src")

        cd %{config.name}

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
      source = {
        "%{config.name}" = {
          hash = config.cargo_hash,
          includes = [
            "Cargo.lock",
            "Cargo.toml",
            "cli/Cargo.toml",
            "notary/Cargo.toml",
            "schema/Cargo.toml",
            "store/Cargo.toml",
            "worker/Cargo.toml"
          ],
          uri = config.source,
        },
      },
      systems = config.systems,
    } | Package
  in

  {
    name = config.name,
    packages = [cargo, rustc, protoc, cache],
    sandbox = build_sandbox config.target,
    script = m%"
      cd %{config.name}

      mkdir -p .cargo
      cp $%{std.string.replace "-" "_" config.name}_cache/config.toml .cargo/config.toml

      cargo build --offline --release
      cargo test --offline --release

      mkdir -p $output/bin
      cp -pr target/release/%{config.name} $output/bin/%{config.name}
    "%,
    source = {
      "%{config.name}" = {
        excludes =
          std.array.concat
            config.source_excludes
            [
              ".git",
              ".gitignore",
              ".vorpal",
              "target",
              "vorpal.lock.ncl",
              "vorpal.ncl",
            ],
        uri = config.source,
      }
    },
    systems = config.systems,
  } | Package
