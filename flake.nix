{
  description = "rose";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    { self
    , nixpkgs
    , flake-utils
    }:
    flake-utils.lib.eachDefaultSystem (system:
    let
      pkgs = import nixpkgs { inherit system; };
      python-pin = pkgs.python312;
      version = nixpkgs.lib.strings.removeSuffix "\n" (builtins.readFile ./rose-py/rose/.version);
      uuid6 = python-pin.pkgs.buildPythonPackage {
        pname = "uuid6-python";
        version = "2023.5.2";
        src = pkgs.fetchFromGitHub {
          owner = "oittaa";
          repo = "uuid6-python";
          rev = "d65fff8bbfcd0bca78577b3d07cb3c9979cd69e7";
          hash = "sha256-Typif9Ags1Eaz2WMCh+MnsbTqJdTPgYpCCReQY8pVqI=";
        };
        pyproject = true;
        build-system = [ python-pin.pkgs.setuptools ];
        doCheck = false;
      };
      py-deps = with python-pin.pkgs; {
        inherit
          # Runtime deps.
          appdirs
          click
          jinja2
          llfuse
          mutagen
          send2trash
          setuptools
          tomli-w
          uuid6
          watchdog
          # Dev tools.
          mypy
          pytest
          pytest-timeout
          pytest-cov
          pytest-xdist
          syrupy;
      };
      python-with-deps = python-pin.withPackages (_:
        pkgs.lib.attrsets.mapAttrsToList (a: b: b) py-deps
      );
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
          export PYTHONPATH="$ROSE_ROOT/rose-py:''${PYTHONPATH:-}"
          export PYTHONPATH="$ROSE_ROOT/rose-watch:$PYTHONPATH"
          export PYTHONPATH="$ROSE_ROOT/rose-vfs:$PYTHONPATH"
          export PYTHONPATH="$ROSE_ROOT/rose-cli:$PYTHONPATH"
        '';
        buildInputs = [
          (pkgs.buildEnv {
            name = "rose-devshell";
            paths = [
              pkgs.ruff
              pkgs.nodePackages.prettier
              pkgs.pyright
              python-with-deps
              # Rust toolchain
              pkgs.rustc
              pkgs.cargo
              pkgs.rustfmt
              pkgs.clippy
              pkgs.rust-analyzer
            ];
          })
        ];
        propagatedBuildInputs = [
          (pkgs.lib.optional pkgs.stdenv.isDarwin pkgs.macfuse-stubs)
        ];
      };
      packages = rec {
        rose-py = pkgs.callPackage ./rose-py { inherit version python-pin py-deps; };
        rose-watch = pkgs.callPackage ./rose-watch { inherit version python-pin py-deps rose-py; };
        rose-vfs = pkgs.callPackage ./rose-vfs { inherit version python-pin py-deps rose-py; };
        rose-cli = pkgs.callPackage ./rose-cli { inherit version python-pin py-deps rose-py rose-vfs rose-watch; };
        rose-rs = pkgs.callPackage ./rose-rs { inherit version; };
        all = pkgs.buildEnv { name = "rose-all"; paths = [ rose-py rose-watch rose-vfs rose-cli rose-rs ]; };
      };
    });
}
