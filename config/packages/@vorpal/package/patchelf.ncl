let { MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let build_package = import "@vorpal/package.ncl" in

fun system =>
  {
    name = "patchelf",
    script | default
      = m%"
        cd ${PWD}/patchelf
        ./bootstrap.sh
        ./configure --prefix=$output
        make -j$(%{MakeCpus system})
        make install
      "%,
    source = {
      patchelf = {
        hash = "a278eec544da9f0a82ad7e07b3670cf0f4d85ee13286fa9ad4f4416b700ac19d",
        strip_prefix = true,
        uri = "https://github.com/NixOS/patchelf/releases/download/0.18.0/patchelf-0.18.0.tar.gz",
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
