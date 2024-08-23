let { Config, .. } = import "schema.ncl" in
let { RustPackage, .. } = import "language.ncl" in

fun system =>
  {
    packages = {
      default =
        RustPackage
          {
            cargo_hash = "c837b047613462710bb72ef7c5a004aa495bd51e29e3369a3d316f689b4f2666",
            name = "vorpal",
            source = ".",
            systems = ["aarch64-linux", "x86_64-linux"],
            target = system
          }
    }
  } | Config
