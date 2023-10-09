{
  description = "rose";

  inputs = {
    nixpkgs.url = github:nixos/nixpkgs/nixos-unstable;
    flake-utils.url = github:numtide/flake-utils;
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    }:
    flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs { inherit system; };
      python = pkgs.python311;
    in
    {
      devShells.default = pkgs.mkShell {
        shellHook = ''
          find-up () {
            path=$(pwd)
            while [[ "$path" != "" && ! -e "$path/$1" ]]; do
              path=''${path%/*}
            done
            echo "$path"
          }
          export ROSE_ROOT="$(find-up .root)"

          # We intentionally do not allow installing Python packages to the
          # global Python environment. Mutable Python installations should be
          # handled via a virtualenv.
          export PIP_CONFIG_FILE="$ROSE_ROOT"/.pip
        '';
        buildInputs = [
          (pkgs.buildEnv {
            name = "rose-devshell";
            paths = with pkgs; [
              (python.withPackages (p: [
                p.mutagen
                p.fuse
              ]))
            ];
          })
        ];
      };
      packages = { };
    });
}
