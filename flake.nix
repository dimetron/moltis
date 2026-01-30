{
  description = "Moltis - Rust version of moltbot";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
      in
      {
        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "moltis";
          version = "0.1.0";
          src = ./.;
          cargoBuildFlags = [ "-p" "moltis-cli" ];
          cargoLock.lockFile = ./Cargo.lock;

          meta = with pkgs.lib; {
            description = "Rust version of moltbot";
            homepage = "https://docs.molt.bot/";
            license = licenses.mit;
            mainProgram = "moltis";
          };
        };

        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            cargo
            rustc
            rust-analyzer
            clippy
            rustfmt
          ];
        };
      }
    );
}
