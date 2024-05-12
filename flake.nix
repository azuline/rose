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
      python-pin = pkgs.python311;
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
        doCheck = false;
      };
      py-deps = with python-pin.pkgs; {
        inherit
          # Runtime deps.
          appdirs
          cffi
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
          snapshottest;
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
          export ROSE_SO_PATH="$ROSE_ROOT/rose-zig/zig-out/lib/librose.so"
          export PYTHONPATH="$ROSE_ROOT/rose-py:''${PYTHONPATH:-}"
          export PYTHONPATH="$ROSE_ROOT/rose-watchdog:$PYTHONPATH"
          export PYTHONPATH="$ROSE_ROOT/rose-vfs:$PYTHONPATH"
          export PYTHONPATH="$ROSE_ROOT/rose-cli:$PYTHONPATH"
        '';
        buildInputs = [
          (pkgs.buildEnv {
            name = "rose-devshell";
            paths = with pkgs; [
              python-with-deps
              ruff
              nodePackages.pyright
              nodePackages.prettier
              zig
              zls
            ];
          })
        ];
      };
      packages = rec {
        rose-zig = pkgs.callPackage ./rose-zig { inherit version; };
        rose-py = pkgs.callPackage ./rose-py { inherit version python-pin py-deps rose-zig; };
        rose-watchdog = pkgs.callPackage ./rose-watchdog { inherit version python-pin py-deps rose-py; };
        rose-vfs = pkgs.callPackage ./rose-vfs { inherit version python-pin py-deps rose-py; };
        rose-cli = pkgs.callPackage ./rose-cli { inherit version python-pin py-deps rose-py rose-vfs rose-watchdog; };
        all = pkgs.buildEnv {
          name = "rose-all";
          paths = [ rose-zig rose-py rose-watchdog rose-vfs rose-cli ];
        };
      };
    });
}
