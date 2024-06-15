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
        inherit (pkgs) alejandra clippy darwin grpcurl just lib openssl pkg-config protobuf rustfmt rustPlatform;
        inherit (darwin.apple_sdk.frameworks) CoreServices SystemConfiguration Security;
        inherit (rustPlatform) buildRustPackage;
      in {
        _module.args.pkgs = import inputs.nixpkgs {
          inherit system;
          overlays = [inputs.rust-overlay.overlays.default];
        };

        apps = {
          default = {
            program = "${config.packages.default}/bin/vorpal";
            type = "app";
          };
        };

        devShells = {
          default = pkgs.mkShell {
            inputsFrom = [config.packages.default];
            nativeBuildInputs = [clippy grpcurl just rustfmt];
          };
        };

        formatter = alejandra;

        packages = {
          default = buildRustPackage {
            buildInputs = [openssl] ++ lib.optionals pkgs.stdenv.isDarwin [CoreServices SystemConfiguration Security];
            cargoSha256 = "sha256-3iH7R+yG9uRQTwFjf+9KPjjlgqk9e6orIhoSHY0LYR0=";
            nativeBuildInputs = [pkg-config protobuf];
            pname = "vorpal";
            src = ./.;
            version = "0.1.0";
          };
        };

        process-compose.start = {
          settings.processes = {
            proxy-server.command = "${config.apps.default.program} services proxy";
            build-server.command = "${config.apps.default.program} services build";
          };
        };
      };
    };
}
