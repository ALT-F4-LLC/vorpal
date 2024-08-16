let { Config, .. } = import "schema.ncl" in
let { PackageRust, .. } = import "language.ncl" in
{
  packages = {
    default =
      PackageRust
        {
          name = "vorpal",
          source = ".",
          systems = ["Aarch64Linux", "X8664Linux"],
          target = "Aarch64Linux",
        }
  }
} | Config
