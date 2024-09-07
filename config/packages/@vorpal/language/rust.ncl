let { Package, .. } = import "@vorpal/contract.ncl" in

let build_cargo = import "@vorpal/package/cargo.ncl" in
let build_protoc = import "@vorpal/package/protoc.ncl" in
let build_rustc = import "@vorpal/package/rustc.ncl" in

fun config =>
  let cargo = build_cargo config.target in
  let rustc = build_rustc config.target in
  let protoc = build_protoc config.target in

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
      source_includes = [
        "Cargo.lock",
        "Cargo.toml",
        "cli/Cargo.toml",
        "notary/Cargo.toml",
        "schema/Cargo.toml",
        "store/Cargo.toml",
        "worker/Cargo.toml"
      ],
      systems = config.systems,
    } | Package
  in

  {
    name = config.name,
    packages = [cargo, rustc, protoc, cache],
    script = m%"
      export LD_LIBRARY_PATH="$rustc/lib:/lib"
      export RUSTFLAGS="-C linker=gcc -L $rustc/lib:/lib"

      mkdir -p .cargo
      cp $%{std.string.replace "-" "_" config.name}_cache/config.toml .cargo/config.toml

      cargo build --offline --release
      cargo test --offline --release

      mkdir -p $output/bin
      cp -pr target/release/%{config.name} $output/bin/%{config.name}
    "%,
    source = config.source,
    source_excludes =
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
    systems = config.systems,
  } | Package
