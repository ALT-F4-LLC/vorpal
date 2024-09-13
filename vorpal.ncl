let build_config = import "@vorpal/config.ncl" in

let build_package = import "@vorpal/language/rust.ncl" in

let build_default = fun system =>
  build_package
    {
      cargo_hash = "4d085d7de342eb39acdb9386663bbea603c45339649ad74857f1909edd088134",
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
