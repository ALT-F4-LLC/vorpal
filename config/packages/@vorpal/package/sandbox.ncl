let build_bash = import "@vorpal/package/bash.ncl" in
let build_binutils = import "@vorpal/package/binutils.ncl" in
let build_coreutils = import "@vorpal/package/coreutils.ncl" in
let build_gcc = import "@vorpal/package/gcc.ncl" in
let build_glibc = import "@vorpal/package/glibc.ncl" in
let build_linux_headers = import "@vorpal/package/linux-headers.ncl" in
let build_m4 = import "@vorpal/package/m4.ncl" in
let build_package = import "@vorpal/package.ncl" in
let build_patchelf = import "@vorpal/package/patchelf.ncl" in
let build_perl = import "@vorpal/package/perl.ncl" in
let build_texinfo = import "@vorpal/package/texinfo.ncl" in
let build_zlib = import "@vorpal/package/zlib.ncl" in
let build_zstd = import "@vorpal/package/zstd.ncl" in

fun system =>
  let bash = build_bash system in
  let binutils = build_binutils system in
  let coreutils = build_coreutils system in
  let gcc = build_gcc system in
  let glibc = build_glibc system in
  let linux_headers = build_linux_headers system in
  let m4 = build_m4 system in
  let patchelf = build_patchelf system in
  let perl = build_perl system in
  let texinfo = build_texinfo system in
  let zlib = build_zlib system in
  let zstd = build_zstd system in

  let packages_common = [
    bash,
    coreutils,
    zstd
  ]
  in

  let packages_system_linux =
    std.array.concat
      packages_common
      [
        binutils,
        gcc,
        glibc,
        linux_headers,
        m4,
        patchelf,
        perl,
        texinfo,
        zlib,
      ]
  in

  let packages_system_macos = std.array.concat packages_common [] in

  let packages_system =
    system
    |> match {
      "aarch64-linux" => packages_system_linux,
      "aarch64-macos" => packages_system_macos,
      "x86_64-linux" => packages_system_linux,
      "x86_64-macos" => packages_system_macos,
      _ => std.fail_with "Unsupported target"
    }
  in

  {
    name = "vorpal-sandbox",
    packages = packages_system,
    script | default
      = m%"
        mkdir -p $output/bin

        cat << "EOF" > $output/bin/sandbox.sh
        #!$bash/bin/bash
        export CC="gcc"
        export LC_ALL="C"
        "${@}"
        EOF

        chmod +x $output/bin/sandbox.sh
      "%,
    systems = [
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  }
  |> build_package
