let { Package, .. } = import "@vorpal/contract.ncl" in

let build_binutils = import "@vorpal/package/binutils.ncl" in

fun target =>
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
    name = "gcc",
    packages = [build_binutils target],
    script = m%"
      pushd "${PWD}/gcc"

      ./contrib/download_prerequisites

      case $(uname -m) in
        x86_64)
          sed -e '/m64=/s/lib64/lib/' -i.orig gcc/config/i386/t-linux64
       ;;
      esac

      mkdir -p build

      popd

      pushd "${PWD}/gcc/build"

      ../configure \
        --disable-bootstrap \
        --disable-fixincludes \
        --disable-multilib \
        --enable-default-pie \
        --enable-default-ssp \
        --enable-host-pie \
        --enable-languages=c,c++ \
        --prefix=$output \
        --with-system-zlib

      make -j$(%{script_build_cpus})

      make install

      popd
    "%,
    source = {
      gcc = {
        hash = "cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz",
      }
    },
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
