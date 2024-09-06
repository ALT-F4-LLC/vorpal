let { Package, .. } = import "@vorpal/contract.ncl" in

let build_rust_std = import "@vorpal/package/rust-std.ncl" in

fun target =>
  let rust_std = build_rust_std target in

  let hash =
    target
    |> match {
      "aarch64-linux" => "bc6c0e0f309805c4a9b704bbfe6be6b3c28b029ac6958c58ab5b90437a9e36ed",
      "aarch64-macos" => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
      "x86_64-linux" => "1512db881f5bdd7f4bbcfede7f5217bd51ca03dc6741c3577b4d071863690211",
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

  {
    name = "rustc",
    packages = [rust_std],
    script = m%"
        cp -pr rustc-%{version}-%{system}/rustc/* $output/.
        cat $rust_std/manifest.in >> $output/manifest.in
        cp -pr $rust_std/lib $output
      "%,
    source = "https://static.rust-lang.org/dist/2024-05-02/rustc-%{version}-%{system}.tar.gz",
    source_hash = hash,
    systems = ["aaarch64-linux", "aarch64-macos", "x86_64-linux"],
  } | Package
