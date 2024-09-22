let { Package, .. } = import "@vorpal/contract.ncl" in

let build_binutils = import "@vorpal/package/binutils.ncl" in
let build_gcc = import "@vorpal/package/gcc.ncl" in
let build_glibc = import "@vorpal/package/glibc.ncl" in

fun target =>
  let binutils = build_binutils target in
  let gcc = build_gcc target in
  let glibc = build_glibc target in

  let make_cpus =
    target
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      "x86_64-linux" => "nproc",
      _ => std.fail_with "Unsupported target"
    }
  in

  {
    name = "bash",
    script = m%"
      pushd "${PWD}/bash"
      ./configure --prefix=$output
      make -j$(%{make_cpus})
      make install
    "%,
    source = {
      bash = {
        hash = "7e3fb70a22919015dfda7602317daa86dc66afa8eb60b99a8dd9d1d8decff662",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/bash/bash-5.2.tar.gz",
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package