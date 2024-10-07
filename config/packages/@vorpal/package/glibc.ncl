let { MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let build_binutils = import "@vorpal/package/binutils.ncl" in
let build_gcc = import "@vorpal/package/gcc.ncl" in
let build_linux_headers = import "@vorpal/package/linux-headers.ncl" in
let build_package = import "@vorpal/package.ncl" in

fun system =>
  let binutils = build_binutils system in
  let gcc = build_gcc system in
  let linux_headers = build_linux_headers system in

  {
    name = "glibc",
    packages = [binutils, gcc, linux_headers],
    script | default
      = m%"
        mkdir -p ${PWD}/glibc-build
        cd ${PWD}/glibc-build
        ../glibc/configure --prefix="$output" libc_cv_slibdir="$output/lib"
        make -j$(%{MakeCpus system})
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
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  }
  |> build_package
