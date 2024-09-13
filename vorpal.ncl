let build_config = import "@vorpal/config.ncl" in

let build_package = import "@vorpal/language/rust.ncl" in

let build_default = fun system =>
  build_package
    {
      cargo_hash = "a7e6b32589e97e78ec8c494ece841a6d7cfa904465b9a058653980cbb87c9ac0",
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
      default = build_default system,
    }
  }
in

fun system => build_config (config system)
