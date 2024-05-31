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
        inherit (pkgs) grpcurl just openssl pkg-config protobuf rustPlatform;
        inherit (rustPlatform) buildRustPackage;
      in {
        packages = {
          default = buildRustPackage {
            cargoSha256 = "sha256-llvbfjVZbrPkWEdIpPJEYLm++HxwN7WypUeZ/RrZtZw=";
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
          vorpal-build = {
            program = "${config.packages.default}/bin/vorpal-build";
            type = "app";
          };

          vorpal-cli = {
            program = "${config.packages.default}/bin/vorpal-cli";
            type = "app";
          };
        };
      };
    };
}
