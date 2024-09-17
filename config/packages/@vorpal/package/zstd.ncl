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
    name = "zstd",
    script = m%"
      pushd "${PWD}/zstd"
      make -j$(%{script_build_cpus})
      make install PREFIX="$output"
      popd
    "%,
    source = {
      zstd = {
        hash = "7ad49c2d889f82744c95206e9fe38ecde31fc116d1dad8def68776c69887efcf",
        strip_prefix = true,
        uri = "https://github.com/facebook/zstd/releases/download/v1.5.5/zstd-1.5.5.tar.gz"
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
