let { Config, .. } = import "@vorpal/schema.ncl" in
let { RustPackage, .. } = import "@vorpal/language.ncl" in

fun system =>
  {
    packages = {
      default =
        RustPackage
          {
            cargo_hash = "478d12c3f1ffcefbc60942308826146326979a22f3f842656e2e1f206e10a45e",
            name = "vorpal",
            source = ".",
            systems = ["aarch64-linux", "x86_64-linux"],
            target = system
          }
    }
  } | Config
