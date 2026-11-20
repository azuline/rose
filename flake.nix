{
  description = "music";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      flake-utils,
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = import nixpkgs { inherit system; };
        python-pin = pkgs.python313;
        version = nixpkgs.lib.strings.removeSuffix "\n" (builtins.readFile ./music-py/music/.version);
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
            syrupy
            ;
        };
        python-with-deps = python-pin.withPackages (_: pkgs.lib.attrsets.mapAttrsToList (a: b: b) py-deps);
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
            export MUSIC_ROOT="$(find-up flake.nix)"
            export PYTHONPATH="$MUSIC_ROOT/music-py:''${PYTHONPATH:-}"
            export PYTHONPATH="$MUSIC_ROOT/music-watch:$PYTHONPATH"
            export PYTHONPATH="$MUSIC_ROOT/music-vfs:$PYTHONPATH"
            export PYTHONPATH="$MUSIC_ROOT/music-cli:$PYTHONPATH"
          '';
          buildInputs = [
            (pkgs.buildEnv {
              name = "music-devshell";
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
          music-py = pkgs.callPackage ./music-py { inherit version python-pin py-deps; };
          music-watch = pkgs.callPackage ./music-watch {
            inherit
              version
              python-pin
              py-deps
              music-py
              ;
          };
          music-vfs = pkgs.callPackage ./music-vfs {
            inherit
              version
              python-pin
              py-deps
              music-py
              ;
          };
          music-cli = pkgs.callPackage ./music-cli {
            inherit
              version
              python-pin
              py-deps
              music-py
              music-vfs
              music-watch
              ;
          };
          all = pkgs.buildEnv {
            name = "music-all";
            paths = [
              music-py
              music-watch
              music-vfs
              music-cli
            ];
          };
        };
      }
    );
}
