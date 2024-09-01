let { Config, .. } = import "@vorpal/schema.ncl" in
let { RustPackage, .. } = import "@vorpal/language.ncl" in

fun system => {
  packages = {
    default = RustPackage {
      cargo_hash = "964e22b0c14bb70ff4c66b5597e1bb5e02558185791ede0eeff835d12ca7a4bb",
      name = "vorpal",
      source = ".",
      systems = ["aarch64-linux", "x86_64-linux"],
      target = system
    }
  }
} | Config
