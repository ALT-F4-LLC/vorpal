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
        inherit (pkgs) just ocamlPackages mkShell;
        inherit (ocamlPackages) buildDunePackage bos digestif mirage-crypto ppxlib ppx_deriving ocamlformat ocaml-lsp utop;
      in {
        devShells = {
          default = mkShell {
            inputsFrom = [config.packages.default];
            nativeBuildInputs = [just];
          };
        };

        packages = {
          default = buildDunePackage {
            pname = "vorpal";
            propagatedBuildInputs =  [bos digestif mirage-crypto ppxlib ppx_deriving ocaml-lsp ocamlformat utop];
            src = ./.;
            version = "0.1.0";
          };
        };
      };
    };
}
