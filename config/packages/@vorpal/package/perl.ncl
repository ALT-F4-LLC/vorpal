let { MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  {
    name = "perl",
    script = m%"
      pushd "${PWD}/perl"
      ./Configure -des -Dprefix="$output"
      make -j$(%{MakeCpus target})
      make install
    "%,
    source = {
      perl = {
        hash = "59b6437a3da1d9de0126135b31f1f16aee9c3b7a0f61f6364b2da3e8bb5f771f",
        strip_prefix = true,
        uri = "https://www.cpan.org/src/5.0/perl-5.40.0.tar.gz",
      }
    },
    systems = [
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
