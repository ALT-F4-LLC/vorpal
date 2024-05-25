{
  description = "vorpal-builder";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = inputs @ {flake-parts, ...}:
    flake-parts.lib.mkFlake {inherit inputs;} {
      systems = ["x86_64-linux" "aarch64-linux" "aarch64-darwin" "x86_64-darwin"];

      perSystem = {
        config,
        pkgs,
        ...
      }: let
        inherit (pkgs) just rustPlatform;
        inherit (rustPlatform) buildRustPackage;
      in {
        devShells = {
          default = pkgs.mkShell {
            inputsFrom = [config.packages.default];
            nativeBuildInputs = [just];
          };
        };

        packages = {
          default = buildRustPackage {
            cargoSha256 = "sha256-v09mDfaCHwePtRMoWXQ56+wcICLUneY5zco1W6lzzL8=";
            pname = "vorpal-builder";
            src = ./.;
            version = "0.1.0";
          };
        };
      };
    };
}
