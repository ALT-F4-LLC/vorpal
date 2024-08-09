let { Config, .. } = import "schema.ncl" in
let { PackageRust, .. } = import "language.ncl" in
{
  packages = {
    default =
      PackageRust
        {
          name = "example",
          source = ".",
          systems = ["X8664Linux"],
          target = "X8664Linux",
        }
  }
} | Config
