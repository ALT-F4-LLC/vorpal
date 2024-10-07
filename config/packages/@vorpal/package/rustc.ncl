let build_package = import "@vorpal/package.ncl" in
let build_rust_std = import "@vorpal/package/rust-std.ncl" in
let build_sandbox = import "@vorpal/package/sandbox.ncl" in

fun system =>
  let rust_std = system |> build_rust_std in

  let hash_dist =
    system
    |> match {
      "aarch64-linux" => "bc6c0e0f309805c4a9b704bbfe6be6b3c28b029ac6958c58ab5b90437a9e36ed",
      "aarch64-macos" => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
      "x86_64-linux" => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
      _ => std.fail_with "Invalid 'rustc' dist"
    }
  in

  let target =
    system
    |> match {
      "aarch64-linux" => "aarch64-unknown-linux-gnu",
      "aarch64-macos" => "aarch64-apple-darwin",
      "x86_64-linux" => "x86_64-unknown-linux-gnu",
      _ => std.fail_with "Invalid 'rustc' target"
    }
  in

  let version = "1.78.0" in

  {
    name = "rustc",
    packages = [rust_std],
    sandbox = system |> build_sandbox,
    script | default
      = m%"
        cp -pr ./dist/rustc/* $output/.
        cat $rust_std/manifest.in >> $output/manifest.in
        cp -pr $rust_std/lib $output
      "%,
    source = {
      dist = {
        hash = hash_dist,
        strip_prefix = true,
        uri = "https://static.rust-lang.org/dist/2024-05-02/rustc-%{version}-%{target}.tar.gz",
      },
    },
    systems = ["aaarch64-linux", "aarch64-macos", "x86_64-linux"],
  }
  |> build_package
