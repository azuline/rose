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
      uuid6-python = python.pkgs.buildPythonPackage {
        pname = "uuid6-python";
        version = "2023.5.2";
        src = pkgs.fetchFromGitHub {
          owner = "oittaa";
          repo = "uuid6-python";
          rev = "d65fff8bbfcd0bca78577b3d07cb3c9979cd69e7";
          hash = "sha256-Typif9Ags1Eaz2WMCh+MnsbTqJdTPgYpCCReQY8pVqI=";
        };
        doCheck = false;
      };
      prod-deps = with python.pkgs; [
        click
        fuse
        mutagen
        uuid6-python
      ];
      dev-deps = with python.pkgs; [
        black
        flake8
        mypy
        pytest
        pytest-cov
        setuptools
      ];
      dev-cli = pkgs.writeShellScriptBin "rose" ''
        cd $ROSE_ROOT
        python -m rose $@
      '';
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
          export ROSE_ROOT="$(find-up flake.nix)"

          # We intentionally do not allow installing Python packages to the
          # global Python environment. Mutable Python installations should be
          # handled via a virtualenv.
          export PIP_CONFIG_FILE="$ROSE_ROOT"/.pip
        '';
        buildInputs = [
          (pkgs.buildEnv {
            name = "rose-devshell";
            paths = with pkgs; [
              (python.withPackages (_: prod-deps ++ dev-deps))
              ruff
              dev-cli
              nodePackages.pyright
            ];
          })
        ];
      };
      packages = rec {
        rose = python.pkgs.buildPythonPackage {
          pname = "rose";
          version = "0.0.0";
          src = ./.;
          propagatedBuildInputs = prod-deps;
        };
        default = rose;
      };
    });
}
