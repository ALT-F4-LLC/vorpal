let { Config, .. } = import "schema.ncl" in
let { RustPackage, .. } = import "language.ncl" in

fun system =>
  {
    packages = {
      default =
        RustPackage
          {
            cargo_hash = "4baecaffa7273beac912e3f4995df69fb1470f0119c2d1a42ed0f7671b8253ee",
            name = "vorpal",
            source = ".",
            systems = ["aarch64-linux", "x86_64-linux"],
            target = system
          }
    }
  } | Config
