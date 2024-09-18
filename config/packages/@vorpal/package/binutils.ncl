let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  let script_build_cpus =
    target
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      _ => std.fail_with "Unsupported target"
    }
  in

  {
    name = "binutils",
    script = m%"
      pushd "${PWD}/binutils"
      ./configure --prefix=$output
      make -j$(%{script_build_cpus})
      make install
    "%,
    source = {
      binutils = {
        hash = "c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/binutils/binutils-2.43.1.tar.gz",
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
