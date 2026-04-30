{
  description = "rose";

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

        # ---- Rust binary builds (rose-cli, rose-vfs) ----
        #
        # We use rustPlatform.buildRustPackage. The workspace is at rose-rs/.
        # rusqlite uses bundled SQLite (no external dep).
        # fuser (rose-vfs) links against libfuse.
        commonRustArgs = {
          pname = "rose";
          version = "0.5.0";
          src = ./rose-rs;
          cargoLock = {
            lockFile = ./rose-rs/Cargo.lock;
          };
          nativeBuildInputs = [ pkgs.pkg-config ];
          buildInputs = [ pkgs.fuse ] ++ pkgs.lib.optional pkgs.stdenv.isDarwin pkgs.macfuse-stubs;
        };

        rose-cli-rs = pkgs.rustPlatform.buildRustPackage (
          commonRustArgs
          // {
            pname = "rose-cli";
            cargoBuildFlags = [
              "-p"
              "rose-cli"
            ];
            cargoTestFlags = [
              "-p"
              "rose-cli"
              "-p"
              "rose-core"
            ];
          }
        );

        rose-vfs-rs = pkgs.rustPlatform.buildRustPackage (
          commonRustArgs
          // {
            pname = "rose-vfs";
            cargoBuildFlags = [
              "-p"
              "rose-vfs"
            ];
            cargoTestFlags = [
              "-p"
              "rose-vfs"
            ];
          }
        );

        # ---- Legacy Python builds (TODO: Remove after Python deletion — task 051) ----
        python-pin = pkgs.python313;
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
            export ROSE_ROOT="$(find-up flake.nix)"
            # Legacy Python paths (TODO: Remove after Python deletion — task 051)
            export PYTHONPATH="$ROSE_ROOT/rose-py:''${PYTHONPATH:-}"
            export PYTHONPATH="$ROSE_ROOT/rose-watch:$PYTHONPATH"
            export PYTHONPATH="$ROSE_ROOT/rose-vfs:$PYTHONPATH"
            export PYTHONPATH="$ROSE_ROOT/rose-cli:$PYTHONPATH"
          '';
          buildInputs = [
            (pkgs.buildEnv {
              name = "rose-devshell";
              paths = [
                # Legacy Python tools (TODO: Remove after Python deletion — task 051)
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
                # Native build deps for cargo build (fuser needs libfuse, pkg-config)
                pkgs.pkg-config
              ];
            })
          ];
          propagatedBuildInputs = [
            pkgs.fuse
            (pkgs.lib.optional pkgs.stdenv.isDarwin pkgs.macfuse-stubs)
          ];
        };
        packages = rec {
          # ---- Rust packages ----
          rose-cli = rose-cli-rs;
          rose-vfs = rose-vfs-rs;

          # ---- Legacy Python packages (TODO: Remove after Python deletion — task 051) ----
          rose-py = pkgs.callPackage ./rose-py { inherit version python-pin py-deps; };
          rose-watch = pkgs.callPackage ./rose-watch {
            inherit
              version
              python-pin
              py-deps
              rose-py
              ;
          };
          rose-vfs-py = pkgs.callPackage ./rose-vfs {
            inherit
              version
              python-pin
              py-deps
              rose-py
              ;
          };
          rose-cli-py = pkgs.callPackage ./rose-cli {
            inherit
              version
              python-pin
              py-deps
              rose-py
              rose-vfs-py
              rose-watch
              ;
          };

          all = pkgs.buildEnv {
            name = "rose-all";
            paths = [
              rose-cli
              rose-vfs
            ];
          };
        };
      }
    );
}
