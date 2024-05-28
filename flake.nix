{
  description = "vorpal";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin"];

      perSystem = {
        config,
        pkgs,
        ...
      }: let
        inherit (pkgs) grpcurl just protobuf rustPlatform;
        inherit (rustPlatform) buildRustPackage;
      in {
        apps = {
          vorpal-build = {
            program = "${config.packages.vorpal}/bin/vorpal-build";
            type = "app";
          };

          vorpal-cli = {
            program = "${config.packages.vorpal}/bin/vorpal-cli";
            type = "app";
          };
        };

        devShells = {
          default = pkgs.mkShell {
            nativeBuildInputs = [grpcurl just];
            inputsFrom = [config.packages.default];
          };
        };

        packages = {
          default = buildRustPackage {
            cargoSha256 = "sha256-mI3N/TvD8gNjJYOFZ9nWodfy00DsM007ZBGS563m+3M=";
            nativeBuildInputs = [protobuf];
            pname = "vorpal";
            src = ./.;
            version = "0.1.0";
          };
        };
      };
    };
}
