let build_config = import "@vorpal/config.ncl" in

let build_package = import "@vorpal/language/rust.ncl" in

let default_package = fun system =>
  build_package
    {
      cargo_hash = "b5db8c4422119dd7b91e438f64cfcfaf9fa5ee8f020cbbbe4ed4ed868201c3f1",
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
