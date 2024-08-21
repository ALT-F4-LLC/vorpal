let { Config, .. } = import "schema.ncl" in
let { RustPackage, .. } = import "language.ncl" in
fun system =>
  {
    packages = {
      default =
        RustPackage
          {
            cargo_hash = "4ec151a638d2eb471e80f6a65f709703f66141306030acd9c5c8d1f77f2def37",
            name = "vorpal",
            source = ".",
            systems = ["aarch64-linux", "x86_64-linux"],
            target = system
          }
    }
  } | Config
