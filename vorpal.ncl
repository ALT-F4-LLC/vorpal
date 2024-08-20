let { Config, .. } = import "schema.ncl" in
let { RustPackage, .. } = import "language.ncl" in
fun system =>
  {
    packages = {
      default =
        RustPackage
          {
            name = "vorpal",
            source = ".",
            systems = ["aarch64-linux", "x86_64-linux"],
            target = system
          }
    }
  } | Config
