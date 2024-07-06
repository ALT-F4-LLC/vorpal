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
        inherit (pkgs) alejandra buildah clippy darwin grpcurl just jq lib mkShell openssl pkg-config protobuf runc rustfmt rustPlatform stdenv umoci;
        inherit (darwin.apple_sdk.frameworks) CoreServices SystemConfiguration Security;
        inherit (lib) optionals;
        inherit (rustPlatform) buildRustPackage;
        inherit (stdenv) isDarwin;
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
          default = mkShell {
            inputsFrom = [config.packages.default];
            nativeBuildInputs = [buildah clippy grpcurl jq just runc rustfmt umoci];
          };
        };

        formatter = alejandra;

        packages = {
          default = buildRustPackage {
            buildInputs = [openssl] ++ optionals isDarwin [CoreServices SystemConfiguration Security];
            cargoSha256 = "sha256-xOE7UgrUTxWzdTuD+vatVTCOrT58V+FWB/Lv3M1CWME=";
            nativeBuildInputs = [pkg-config protobuf];
            pname = "vorpal";
            src = ./.;
            version = "0.1.0";
          };
        };

        process-compose.start = {
          settings.processes = {
            agent-server.command = "${config.apps.default.program} services agent";
            worker-server.command = "${config.apps.default.program} services worker";
          };
        };
      };
    };
}
