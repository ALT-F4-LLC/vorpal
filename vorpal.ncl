let { Config, .. } = import "@vorpal/schema.ncl" in
let { RustPackage, .. } = import "@vorpal/language.ncl" in

fun system =>
  {
    packages = {
      default =
        RustPackage
          {
            cargo_hash = "eacfdcf3386a4b9f1a7032d8bed61e0a37df5c860c3e4a7de70ad203779ef447",
            name = "vorpal",
            source = ".",
            systems = ["aarch64-linux", "x86_64-linux"],
            target = system
          }
    }
  } | Config
