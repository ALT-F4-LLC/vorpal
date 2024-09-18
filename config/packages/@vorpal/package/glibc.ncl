let { Package, .. } = import "@vorpal/contract.ncl" in

let build_binutils = import "@vorpal/package/binutils.ncl" in
let build_gcc = import "@vorpal/package/gcc.ncl" in
let build_linux_headers = import "@vorpal/package/linux-headers.ncl" in

fun target =>
  let binutils = build_binutils target in
  let gcc = build_gcc target in
  let linux_headers = build_linux_headers target in

  let script_build_cpus =
    target
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      _ => std.fail_with "Unsupported target"
    }
  in

  {
    name = "glibc",
    packages = [binutils, gcc, linux_headers],
    script = m%"
      mkdir -p "${PWD}/glibc-build"
      pushd "${PWD}/glibc-build"
      ../glibc/configure --prefix="$output" libc_cv_slibdir="$output/lib"
      make -j$(%{script_build_cpus})
      make install
    "%,
    source = {
      glibc = {
        hash = "da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/glibc/glibc-2.40.tar.gz",
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
