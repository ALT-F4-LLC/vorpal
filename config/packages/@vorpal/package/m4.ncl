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
    name = "m4",
    script = m%"
      pushd "${PWD}/m4"
      ./configure --prefix=$output
      make -j$(%{script_build_cpus})
      make install
    "%,
    source = {
      m4 = {
        hash = "fd793cdfc421fac76f4af23c7d960cbe4a29cbb18f5badf37b85e16a894b3b6d",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/m4/m4-1.4.19.tar.gz",
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
