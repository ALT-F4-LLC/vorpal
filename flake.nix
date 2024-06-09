{
  description = "vorpal";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";
  inputs.rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  inputs.rust-overlay.url = "github:oxalica/rust-overlay";

  outputs = inputs @ {
    flake-parts,
    process-compose-flake,
    ...
  }:
    flake-parts.lib.mkFlake {inherit inputs;} {
      imports = [process-compose-flake.flakeModule];

      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin"];

      perSystem = {
        config,
        pkgs,
        system,
        ...
      }: let
        inherit (pkgs) clippy darwin grpcurl just lib openssl pkg-config protobuf rustfmt rustPlatform;
        inherit (darwin.apple_sdk.frameworks) CoreServices SystemConfiguration Security;
        inherit (rustPlatform) buildRustPackage;
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [inputs.rust-overlay.overlays.default];
        };

        packages = {
          default = buildRustPackage {
            buildInputs = [openssl] ++ lib.optionals pkgs.stdenv.isDarwin [CoreServices SystemConfiguration Security];
            cargoSha256 = "sha256-I9yYNZEGJml2MyLK6BshNro7wFpn8MdZpUeGyMJs2o0=";
            checkPhase = ''
              ${pkgs.cargo}/bin/cargo clippy -- -D warnings
              ${pkgs.rust-bin.nightly.latest.default}/bin/cargo fmt --check --verbose
              ${pkgs.cargo}/bin/cargo test --locked --all-features --all-targets
            '';
            nativeBuildInputs = [clippy pkg-config protobuf rustfmt];
            pname = "vorpal";
            src = ./.;
            version = "0.1.0";
          };
        };

        devShells = {
          default = pkgs.mkShell {
            nativeBuildInputs = [grpcurl just];
            inputsFrom = [config.packages.default];
          };
        };

        apps = {
          default = {
            program = "${config.packages.default}/bin/vorpal";
            type = "app";
          };
        };

        process-compose.start-dev = {
          settings.processes = {
            build-server.command = "${config.apps.default.program} service build start";
            proxy-server.command = "${config.apps.default.program} service proxy start";
          };
        };
      };
    };
}
