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
        inherit (pkgs) ocamlPackages mkShell;
        inherit (ocamlPackages) buildDunePackage mirage-crypto;
      in {
        devShells = {
          default = mkShell {
            inputsFrom = [config.packages.default];
            nativeBuildInputs = [mirage-crypto];
          };
        };

        packages = {
          default = buildDunePackage {
            pname = "vorpal";
            propogatedBuildInputs = [mirage-crypto];
            src = ./.;
            version = "0.1.0";
          };
        };
      };
    };
}
