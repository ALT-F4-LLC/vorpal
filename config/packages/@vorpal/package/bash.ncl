let { IsLinux, MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let build_glibc = import "@vorpal/package/glibc.ncl" in
let build_package = import "@vorpal/package.ncl" in
let build_patchelf = import "@vorpal/package/patchelf.ncl" in

fun system =>
  let glibc = system |> build_glibc in
  let patchelf = system |> build_patchelf in

  {
    environment = {
      LC_ALL = "C",
    },
    name = "bash",
    packages = if (IsLinux system) then [glibc, patchelf] else [],
    script | default
      = m%"
      cd ${PWD}/bash
      ./configure --prefix=$output
      make -j$(%{MakeCpus system})
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
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  }
  |> build_package
