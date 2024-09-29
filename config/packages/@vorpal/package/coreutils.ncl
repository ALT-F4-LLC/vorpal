let { MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  {
    name = "coreutils",
    script = m%"
      pushd "${PWD}/coreutils"
      ./configure --prefix="$output"
      make -j$(%{MakeCpus target})
      make install
    "%,
    source = {
      coreutils = {
        hash = "af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/coreutils/coreutils-9.5.tar.gz"
      }
    },
    systems = [
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
