let build_config = import "@vorpal/config.ncl" in

let build_package = import "@vorpal/language/rust.ncl" in

let default_package = fun system =>
  build_package
    {
      cargo_hash = "23ee1efa1633ed7e94976dd46469535bc6bdd9749efa9fb10b0c7cf043f77715",
      name = "vorpal",
      source = ".",
      source_excludes = [
        ".dockerignore",
        ".envrc",
        ".github",
        "Dockerfile",
        "Dockerfile.sandbox",
        "LICENSE",
        "README.md",
        "deps",
        "dev.sh",
        "justfile",
        "nickel",
        "script",
        "vorpal.png",
      ],
      systems = ["aarch64-linux", "x86_64-linux"],
      target = system,
    }
in

let config = fun system =>
  {
    packages = {
      default = default_package system,
    }
  }
in

fun system => build_config (config system)
