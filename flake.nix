{
  description = "vorpal";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin"];

      perSystem = {
        config,
        inputs',
        pkgs,
        system,
        ...
      }: let
        inherit (pkgs) alejandra clippy darwin grpcurl just jq lib mkShell nickel openssl pkg-config protobuf rustfmt rustPlatform stdenv;
        inherit (darwin.apple_sdk.frameworks) CoreServices SystemConfiguration Security;
        inherit (lib) optionals;
        inherit (rustPlatform) buildRustPackage;
        inherit (stdenv) isDarwin;
        pname = "vorpal";
        version = "0.1.0";
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          config.allowUnfree = true;
          overlays = [inputs.rust-overlay.overlays.default];
        };

        apps = {
          default = {
            program = "${config.packages.default}/bin/vorpal";
            type = "app";
          };
        };

        devShells = {
          default = mkShell {
            NICKEL_IMPORT_PATH = ".:./.vorpal/packages";
            inputsFrom = [config.packages.default];
            packages = [
              clippy
              grpcurl
              jq
              just
              nickel
              rustfmt
            ];
          };
        };

        formatter = alejandra;

        packages = {
          default = buildRustPackage {
            inherit pname version;
            buildInputs = [openssl] ++ optionals isDarwin [CoreServices SystemConfiguration Security];
            cargoHash = "sha256-QpfL3JQ+Amzr5TXjmceefY4NZtW0p/C6g9twpWYlN3s=";
            nativeBuildInputs = [pkg-config protobuf];
            src = ./.;
          };
        };
      };
    };
}
