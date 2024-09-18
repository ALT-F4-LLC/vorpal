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
    name = "zlib",
    script = m%"
      pushd "${PWD}/zlib"
      ./configure --prefix=$output
      make -j$(%{script_build_cpus})
      make install
    "%,
    source = {
      zlib = {
        hash = "3f7995d5f103719283f509c23624287ce95c349439e881ed935a3c2c807bb683",
        strip_prefix = true,
        uri = "https://github.com/madler/zlib/releases/download/v1.3.1/zlib-1.3.1.tar.gz",
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
