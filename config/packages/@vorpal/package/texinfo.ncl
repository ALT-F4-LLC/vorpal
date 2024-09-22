let { Package, .. } = import "@vorpal/contract.ncl" in
let build_perl = import "@vorpal/package/perl.ncl" in

fun target =>
  let perl = build_perl target in

  let script_build_cpus =
    target
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      "x86_64-linux" => "nproc",
      _ => std.fail_with "Unsupported target"
    }
  in

  {
    name = "texinfo",
    script = m%"
      pushd "${PWD}/texinfo"
      ./configure --prefix="$output"
      make -j$(%{script_build_cpus})
      make install
      make TEXMF=$output install-tex
    "%,
    packages = [perl],
    source = {
      texinfo = {
        hash = "6e34604552af91db0b4ccf0bcceba63dd3073da2a492ebcf33c6e188a64d2b63",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/texinfo/texinfo-7.1.1.tar.gz"
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
