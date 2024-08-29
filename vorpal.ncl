let { Config, .. } = import "@vorpal/schema.ncl" in
let { RustPackage, .. } = import "@vorpal/language.ncl" in

fun system =>
  {
    packages = {
      default =
        RustPackage
          {
            cargo_hash = "288faaffe94e2c7b41bcfa68a0e7d689721a5a2b20945dcc16a9ceba3e69b914",
            name = "vorpal",
            source = ".",
            systems = ["aarch64-linux", "x86_64-linux"],
            target = system
          }
    }
  } | Config
