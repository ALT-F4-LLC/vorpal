let build_config = import "@vorpal/config.ncl" in

let build_package = import "@vorpal/language/rust.ncl" in

let default_package = fun system =>
  build_package
    {
      cargo_hash = "0aff48a16911e91a80c7e6dcf855700ef997ca154ef3124a528d361ea5f6a7a1",
      name = "vorpal",
      source = ".",
      source_excludes = [".env", ".packer", ".vagrant"],
      systems = ["aarch64-linux", "x86_64-linux"],
      target = system,
    }
in

let config = fun system =>
  {
    packages = {
      default = default_package system,
    }
  }
in

fun system => build_config (config system)
