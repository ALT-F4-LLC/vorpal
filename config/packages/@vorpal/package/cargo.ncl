let build_package = import "@vorpal/package.ncl" in

fun target =>
  let hash =
    target
    |> match {
      "aarch64-linux" => "d782e34151df01519de86f0acace8a755cae6fad93cb0303ddd61c2642444c1c",
      "aarch64-macos" => "d8ed8e9f5ceefcfe3bca7acd0797ade24eadb17ddccaa319cd00ea290f598d00",
      "x86_64-linux" => "d8ed8e9f5ceefcfe3bca7acd0797ade24eadb17ddccaa319cd00ea290f598d00",
      _ => std.fail_with "Unsupported target"
    }
  in

  let system =
    target
    |> match {
      "aarch64-linux" => "aarch64-unknown-linux-gnu",
      "aarch64-macos" => "aarch64-apple-darwin",
      "x86_64-linux" => "x86_64-unknown-linux-gnu",
      _ => std.fail_with "Unsupported target"
    }
  in

  let version = "1.78.0" in

  build_package
    {
      name = "cargo",
      packages = [],
      script = "cp -pr cargo-%{version}-%{system}/cargo/* $output/.",
      source = "https://static.rust-lang.org/dist/2024-05-02/cargo-%{version}-%{system}.tar.gz",
      source_excludes = [],
      source_hash = hash,
      source_includes = [],
      systems = ["aaarch64-linux", "aarch64-macos", "x86_64-linux"],
    }
