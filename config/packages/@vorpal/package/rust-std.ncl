let build_package = import "@vorpal/package.ncl" in
let build_sandbox = import "@vorpal/package/sandbox.ncl" in

fun system =>
  let hash_dist =
    system
    |> match {
      "aarch64-linux" => "72d4917bb58b693b3f2c589746ed470645f96895ece3dd27f7055d3c3f7f7a79",
      "aarch64-macos" => "0689a9b2dec87c272954db9212a8f3d5243f55f777f90d84d2b3aeb2aa938ba5",
      "x86_64-linux" => "ad734eb9699b0a9dffdd35034776ccaa4d7b45e1898fc32748be93b60453550d",
      _ => std.fail_with "Unsupported 'rust-std' hash_dist"
    }
  in

  let target =
    system
    |> match {
      "aarch64-linux" => "aarch64-unknown-linux-gnu",
      "aarch64-macos" => "aarch64-apple-darwin",
      "x86_64-linux" => "x86_64-unknown-linux-gnu",
      _ => std.fail_with "Unsupported 'rust-std' target"
    }
  in

  let version = "1.78.0" in

  {
    name = "rust-std",
    sandbox = system |> build_sandbox,
    script | default = "cp -pr ./dist/rust-std-%{target}/* $output/.",
    source = {
      dist = {
        hash = hash_dist,
        strip_prefix = true,
        uri = "https://static.rust-lang.org/dist/2024-05-02/rust-std-%{version}-%{target}.tar.gz",
      }
    },
    systems = ["aaarch64-linux", "aarch64-macos", "x86_64-linux"],
  }
  |> build_package
