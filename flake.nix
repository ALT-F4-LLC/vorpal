{
  description = "vorpal";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  inputs.process-compose-flake.url = "github:Platonic-Systems/process-compose-flake";

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
        ...
      }: let
        inherit (pkgs) darwin grpcurl just lib openssl pkg-config protobuf rustPlatform;
        inherit (darwin.apple_sdk.frameworks) CoreServices SystemConfiguration Security;
        inherit (rustPlatform) buildRustPackage;
      in {
        packages = {
          default = buildRustPackage {
            buildInputs = [openssl] ++ lib.optionals pkgs.stdenv.isDarwin [CoreServices SystemConfiguration Security];
            cargoSha256 = "sha256-Qvxhf+lY7Khtt0RP2VASh5CDYeii5KaFX8krDn6QpnA=";
            nativeBuildInputs = [pkg-config protobuf];
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
