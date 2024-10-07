let { Package, .. } = import "@vorpal/contract.ncl" in
let build_binutils = import "@vorpal/package/binutils.ncl" in
let build_gcc = import "@vorpal/package/gcc.ncl" in
let build_package = import "@vorpal/package.ncl" in

fun system =>
  let binutils = build_binutils system in
  let gcc = build_gcc system in

  {
    name = "linux-headers",
    packages = [binutils, gcc],
    script | default
      = m%"
        cd ${PWD}/linux_headers
        make mrproper
        make headers
        find usr/include -type f ! -name '*.h' -delete
        mkdir -p "$output/usr"
        cp -rv usr/include "$output/usr"
      "%,
    source = {
      linux_headers = {
        hash = "3fa3f4f3d010de5b9bde09d08a251fa3ef578d356d3a7a29b6784a6916ea0d50",
        strip_prefix = true,
        uri = "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.10.8.tar.xz"
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
