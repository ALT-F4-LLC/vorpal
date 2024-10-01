let { MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  {
    name = "zstd",
    script = m%"
      cd ${PWD}/zstd
      make -j$(%{MakeCpus target})
      make install PREFIX="$output"
    "%,
    source = {
      zstd = {
        hash = "7ad49c2d889f82744c95206e9fe38ecde31fc116d1dad8def68776c69887efcf",
        strip_prefix = true,
        uri = "https://github.com/facebook/zstd/releases/download/v1.5.5/zstd-1.5.5.tar.gz"
      }
    },
    systems = [
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
