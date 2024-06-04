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
        inherit (pkgs) grpcurl just protobuf rustPlatform;
        inherit (rustPlatform) buildRustPackage;
      in {
        packages = {
          default = buildRustPackage {
            cargoSha256 = "sha256-l8A+eH+UeLd+IccoT4T67wDW0ya5M89tsQKwtQnbwog=";
            nativeBuildInputs = [protobuf];
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
