let { MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  {
    name = "texinfo",
    script = m%"
      cd ${PWD}/texinfo
      ./configure --prefix="$output"
      make -j$(%{MakeCpus target})
      make install
      make TEXMF=$output install-tex
    "%,
    source = {
      texinfo = {
        hash = "6e34604552af91db0b4ccf0bcceba63dd3073da2a492ebcf33c6e188a64d2b63",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/texinfo/texinfo-7.1.1.tar.gz"
      }
    },
    systems = [
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
