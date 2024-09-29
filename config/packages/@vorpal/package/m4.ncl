let { MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  {
    name = "m4",
    script = m%"
      pushd "${PWD}/m4"
      ./configure --prefix=$output
      make -j$(%{MakeCpus target})
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
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
