let { Package, .. } = import "@vorpal/contract.ncl" in

let build_bash = import "@vorpal/package/bash.ncl" in
let build_binutils = import "@vorpal/package/binutils.ncl" in
let build_coreutils = import "@vorpal/package/coreutils.ncl" in
let build_gcc = import "@vorpal/package/gcc.ncl" in
let build_glibc = import "@vorpal/package/glibc.ncl" in
let build_linux_headers = import "@vorpal/package/linux-headers.ncl" in
let build_m4 = import "@vorpal/package/m4.ncl" in
let build_perl = import "@vorpal/package/perl.ncl" in
let build_script = import "@vorpal/package/generic/script.ncl" in
let build_texinfo = import "@vorpal/package/texinfo.ncl" in
let build_zlib = import "@vorpal/package/zlib.ncl" in
let build_zstd = import "@vorpal/package/zstd.ncl" in

fun target =>
  let bash = build_bash target in
  let binutils = build_binutils target in
  let coreutils = build_coreutils target in
  let gcc = build_gcc target in
  let glibc = build_glibc target in
  let linux_headers = build_linux_headers target in
  let m4 = build_m4 target in
  let perl = build_perl target in
  let texinfo = build_texinfo target in
  let zlib = build_zlib target in
  let zstd = build_zstd target in

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
        perl,
        texinfo,
        zlib,
      ]
  in

  let packages_system_macos = std.array.concat packages_common [] in

  let packages_system =
    target
    |> match {
      "aarch64-linux" => packages_system_linux,
      "aarch64-macos" => packages_system_macos,
      "x86_64-linux" => packages_system_linux,
      "x86_64-macos" => packages_system_macos,
      _ => std.fail_with "Unsupported target"
    }
  in

  let package_script =
    build_script
      target
      {
        name = "vorpal-sandbox-script",
        script = m%"
        export LC_ALL="C"
        ${@}
        "%,
      }
  in

  {
    name = "vorpal-sandbox",
    packages = std.array.concat packages_system [package_script],
    script = m%"
      mkdir -p $output/bin
      ln -s $vorpal_sandbox_script/bin/vorpal-sandbox-script $output/bin/sandbox.sh
    "%,
    systems = [
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
