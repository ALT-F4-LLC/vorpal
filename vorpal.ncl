let { Config, .. } = import "@vorpal/schema.ncl" in
let { RustPackage, .. } = import "@vorpal/language.ncl" in

fun system => {
  packages = {
    default = RustPackage {
      cargo_hash = "23ee1efa1633ed7e94976dd46469535bc6bdd9749efa9fb10b0c7cf043f77715",
      name = "vorpal",
      source = ".",
      systems = ["aarch64-linux", "x86_64-linux"],
      target = system
    }
  }
} | Config
