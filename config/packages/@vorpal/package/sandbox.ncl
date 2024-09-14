let { Package, .. } = import "@vorpal/contract.ncl" in

fun target =>
  let script_build_cpus =
    target
    |> match {
      "aarch64-linux" => "nproc",
      "aarch64-macos" => "sysctl -n hw.ncpu",
      _ => std.fail_with "Unsupported target"
    }
  in

  let script_autoconf = m%"
    pushd "${PWD}/autoconf"
    ./configure --prefix="$output"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_automake = m%"
    pushd "${PWD}/automake"
    ./configure --prefix="$output"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_bash = m%"
    pushd "${PWD}/bash"
    ./configure --prefix="$output"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_binutils = m%"
    pushd "${PWD}/binutils"
    ./configure --prefix="$output"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_coreutils = m%"
    pushd "${PWD}/coreutils"
    ./configure --prefix="$output"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_gcc = m%"
    pushd "${PWD}/gcc"
    ./contrib/download_prerequisites
    mkdir -p "${PWD}/gcc-build"
    popd
    pushd "${PWD}/gcc-build"
    ../configure \
        --disable-multilib \
        --enable-languages="c,c++" \
        --prefix="$output"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_glibc = m%"
    mkdir -p "${PWD}/glibc-build"
    pushd "${PWD}/glibc-build"
    ../glibc/configure --prefix="$output" libc_cv_slibdir="$output/lib"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_linux_headers = m%"
    pushd "${PWD}/linux_headers"
    make mrproper
    make headers
    find usr/include -type f ! -name '*.h' -delete
    mkdir -p "$output/usr"
    cp -rv usr/include "$output/usr"
    popd
  "%
  in

  let script_m4 = m%"
    pushd "${PWD}/m4"
    ./configure --prefix="$output"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_perl = m%"
    pushd "${PWD}/perl"
    ./Configure -des -Dprefix="$output"
    make -j$(%{script_build_cpus})
    make install
    popd
  "%
  in

  let script_texinfo = m%"
    pushd "${PWD}/texinfo"
    ./configure --prefix="$output"
    make -j$(%{script_build_cpus})
    make install
    make TEXMF=$output install-tex
    popd
  "%
  in

  let script_zstd = m%"
    pushd "${PWD}/zstd"
    make -j$(%{script_build_cpus})
    make install PREFIX="$output"
    popd
  "%
  in

  let source_autoconf = {
    hash = "1823586c20834bcfaf42bb4d491a2b236419cea57ac680f514ef770ce1b070ce",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/autoconf/autoconf-2.72.tar.gz",
  }
  in

  let source_automake = {
    hash = "a840c6cd0a65a5d1576ba079173345d66188b96e9144a25489a59ed2d04b56cb",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/automake/automake-1.17.tar.gz",
  }
  in

  let source_bash = {
    hash = "7e3fb70a22919015dfda7602317daa86dc66afa8eb60b99a8dd9d1d8decff662",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/bash/bash-5.2.tar.gz",
  }
  in

  let source_binutils = {
    hash = "c0d3e5ee772ee201eefe17544b2b2cc3a0a3d6833a21b9ea56371efaad0c5528",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/binutils/binutils-2.43.1.tar.gz",
  }
  in

  let source_coreutils = {
    hash = "af6d643afd6241ec35c7781b7f999b97a66c84bea4710ad2bb15e75a5caf11b4",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/coreutils/coreutils-9.5.tar.gz",
  }
  in

  let source_gcc = {
    hash = "cc20ef929f4a1c07594d606ca4f2ed091e69fac5c6779887927da82b0a62f583",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/gcc/gcc-14.2.0/gcc-14.2.0.tar.gz",
  }
  in

  let source_glibc = {
    hash = "da2594c64d61dacf80d85e568136bf31fba36c4ff1ececff59c6fb786a2a126b",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/glibc/glibc-2.40.tar.gz",
  }
  in

  let source_linux_headers = {
    hash = "3fa3f4f3d010de5b9bde09d08a251fa3ef578d356d3a7a29b6784a6916ea0d50",
    strip_prefix = true,
    uri = "https://cdn.kernel.org/pub/linux/kernel/v6.x/linux-6.10.8.tar.xz",
  }
  in

  let source_m4 = {
    hash = "fd793cdfc421fac76f4af23c7d960cbe4a29cbb18f5badf37b85e16a894b3b6d",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/m4/m4-1.4.19.tar.gz",
  }
  in

  let source_perl = {
    hash = "59b6437a3da1d9de0126135b31f1f16aee9c3b7a0f61f6364b2da3e8bb5f771f",
    strip_prefix = true,
    uri = "https://www.cpan.org/src/5.0/perl-5.40.0.tar.gz",
  }
  in

  let source_texinfo = {
    hash = "6e34604552af91db0b4ccf0bcceba63dd3073da2a492ebcf33c6e188a64d2b63",
    strip_prefix = true,
    uri = "https://ftp.gnu.org/gnu/texinfo/texinfo-7.1.1.tar.gz",
  }
  in

  let source_zstd = {
    hash = "7ad49c2d889f82744c95206e9fe38ecde31fc116d1dad8def68776c69887efcf",
    strip_prefix = true,
    uri = "https://github.com/facebook/zstd/releases/download/v1.5.5/zstd-1.5.5.tar.gz",
  }
  in

  let script_system_common = m%"
    %{script_m4}
    %{script_autoconf}
    %{script_automake}
    %{script_perl}
    %{script_texinfo}
    %{script_bash}
    %{script_coreutils}
    %{script_zstd}
  "%
  in

  let script_system_linux = m%"
    %{script_binutils}
    %{script_gcc}
    %{script_linux_headers}
    %{script_glibc}
  "%
  in

  let script_system =
    target
    |> match {
      "aarch64-linux" =>
        m%"
          export PATH="$output/bin:$PATH"
          %{script_system_common}
          %{script_system_linux}
      "%,
      "aarch64-macos" =>
        m%"
          export PATH="$output/bin:$PATH"
          %{script_system_common}
      "%,
      _ => std.fail_with "Unsupported target"
    }
  in

  let source_system_common = {
    autoconf = source_autoconf,
    automake = source_automake,
    bash = source_bash,
    coreutils = source_coreutils,
    m4 = source_m4,
    perl = source_perl,
    texinfo = source_texinfo,
    zstd = source_zstd,
  }
  in

  let source_system_linux = {
    binutils = source_binutils,
    gcc = source_gcc,
    glibc = source_glibc,
    linux_headers = source_linux_headers,
  }
  in

  let source_system =
    target
    |> match {
      "aarch64-linux" => std.array.concat source_system_common source_system_linux,
      "aarch64-macos" => source_system_common,
      _ => std.fail_with "Unsupported target"
    }
  in

  {
    name = "vorpal-sandbox",
    script = script_system,
    source = source_system,
    systems = [
      "aaarch64-linux",
      "aarch64-macos",
      "x86_64-linux",
      "x86_64-macos"
    ],
  } | Package
