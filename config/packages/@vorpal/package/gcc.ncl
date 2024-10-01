let { MakeCpus, .. } = import "@vorpal/cross-platform.ncl" in
let { Package, .. } = import "@vorpal/contract.ncl" in

let build_binutils = import "@vorpal/package/binutils.ncl" in

fun target =>
  {
    name = "gcc",
    packages = [build_binutils target],
    script = m%"
      cd ${PWD}/gcc

      ./contrib/download_prerequisites

      case $(uname -m) in
        x86_64)
          sed -e '/m64=/s/lib64/lib/' -i.orig gcc/config/i386/t-linux64
       ;;
      esac

      mkdir -p build

      cd ${PWD}/gcc/build

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

      make -j$(%{MakeCpus target})
      make install
    "%,
    source = {
      gcc = {
        hash = "cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583",
        strip_prefix = true,
        uri = "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz",
      }
    },
    systems = [
      "aarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
