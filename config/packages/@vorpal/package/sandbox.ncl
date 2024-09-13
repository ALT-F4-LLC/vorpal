let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  let source_system =
    target
    |> match {
      "aarch64-macos" =>
        {
          bash = {
            hash = "7e3fb70a22919015dfda7602317daa86dc66afa8eb60b99a8dd9d1d8decff662",
            strip_prefix = true,
            uri = "https://ftp.gnu.org/gnu/bash/bash-5.2.tar.gz",
          },
          coreutils = {
            hash = "af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4",
            strip_prefix = true,
            uri = "https://ftp.gnu.org/gnu/coreutils/coreutils-9.5.tar.gz",
          },
          zstd = {
            hash = "7ad49c2d889f82744c95206e9fe38ecde31fc116d1dad8def68776c69887efcf",
            strip_prefix = true,
            uri = "https://github.com/facebook/zstd/releases/download/v1.5.5/zstd-1.5.5.tar.gz",
          },
        },
      _ => std.fail_with "Unsupported target"
    }
  in

  {
    name = "vorpal-sandbox",
    script = m%"
      pushd "${PWD}/bash"
      ./configure --prefix="$output"
      make
      make install
      popd

      pushd "${PWD}/coreutils"
      ./configure --prefix="$output"
      make
      make install
      popd

      pushd "${PWD}/zstd"
      make
      make install PREFIX="$output"
      popd
  "%,
    source = source_system,
    systems = ["aaarch64-linux", "aarch64-macos", "x86_64-linux"],
  } | Package
