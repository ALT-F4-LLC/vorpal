{pkgs ? import <nixpkgs> {}}: let
  overrides = builtins.fromTOML (builtins.readFile ./rust-toolchain.toml);
  libPath = with pkgs;
    lib.makeLibraryPath [
      # load external libraries that you need in your rust project here
    ];
in
  pkgs.mkShell {
    # Add glibc, clang, glib, and other headers to bindgen search path
    BINDGEN_EXTRA_CLANG_ARGS =
      # Includes normal include path
      (builtins.map (a: ''-I"${a}/include"'') [
        # add dev libraries here (e.g. pkgs.libvmi.dev)
        pkgs.glibc.dev
      ])
      # Includes with special directory paths
      ++ [
        ''-I"${pkgs.llvmPackages_latest.libclang.lib}/lib/clang/${pkgs.llvmPackages_latest.libclang.version}/include"''
        ''-I"${pkgs.glib.dev}/include/glib-2.0"''
        ''-I${pkgs.glib.out}/lib/glib-2.0/include/''
      ];

    # https://github.com/rust-lang/rust-bindgen#environment-variables
    LIBCLANG_PATH = pkgs.lib.makeLibraryPath [pkgs.llvmPackages_latest.libclang.lib];

    RUSTC_VERSION = overrides.toolchain.channel;

    # Add precompiled library to rustc search path
    RUSTFLAGS = builtins.map (a: ''-L ${a}/lib'') [
      # add libraries here (e.g. pkgs.libvmi)
    ];

    buildInputs = with pkgs; [
      # autoconf
      # automake
      # bison
      # flex
      # gettext
      # help2man
      # nickel
      # zlib
      bubblewrap
      protobuf

      clang
      # Replace llvmPackages with llvmPackages_X, where X is the latest LLVM version (at the time of writing, 16)
      llvmPackages.bintools
      gnumake
      rustup
      unzip
    ];

    hardeningDisable = ["all"];

    shellHook = ''
      export PATH=$PATH:''${CARGO_HOME:-~/.cargo}/bin
      export PATH=$PATH:''${RUSTUP_HOME:-~/.rustup}/toolchains/$RUSTC_VERSION-x86_64-unknown-linux-gnu/bin/
      export PATH=$PATH:$PWD/.env/bin
      export NICKEL_IMPORT_PATH="$PWD/.vorpal/packages:$PWD"
    '';
  }
