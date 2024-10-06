let build_config = import "@vorpal/config.ncl" in

let build_rust = import "@vorpal/language/rust.ncl" in

let build_default = fun system =>
  build_rust {
    cargo_hash = "fc3a0a6127c3d33b3d93961e4969edf5099a756166cf6f60499b69b2147639b1",
    name = "vorpal",
    source = ".",
    source_excludes = ["config", ".env", ".packer", ".vagrant"],
    systems = ["aarch64-linux", "aarch64-macos", "x86_64-linux", "x86_64-macos"],
    target = system,
  }
in

let config = fun system =>
  {
    packages = {
      default = build_default system,
    }
  }
in

fun system => build_config (config system)
