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
      prod-py-deps = with python.pkgs; [
        appdirs
        cffi
        click
        jinja2
        llfuse
        mutagen
        send2trash
        setuptools
        tomli-w
        uuid6-python
        watchdog
      ];
      dev-py-deps = with python.pkgs; [
        mypy
        pytest
        pytest-timeout
        pytest-cov
        pytest-xdist
        snapshottest
      ];
      version = nixpkgs.lib.strings.removeSuffix "\n" (builtins.readFile ./rose/.version);
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
        '';
        buildInputs = [
          (pkgs.buildEnv {
            name = "rose-devshell";
            paths = with pkgs; [
              (python.withPackages (_: prod-py-deps ++ dev-py-deps))
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
        rose-zig = pkgs.stdenv.mkDerivation {
          pname = "rose";
          version = version;
          src = ./rose_zig;
          nativeBuildInputs = [ pkgs.zig.hook ];
        };
        # TODO: Split up into multiple packages.
        rose-py = python.pkgs.buildPythonPackage {
          pname = "rose";
          version = version;
          src = ./.;
          propagatedBuildInputs = prod-py-deps ++ [ rose-zig ];
          nativeBuildInputs = [ pkgs.makeWrapper ];
          postInstall = ''
            wrapProgram $out/bin/rose \
              --prefix LD_LIBRARY_PATH : "${pkgs.lib.makeLibraryPath [ rose-zig ]}"
          '';
          doCheck = false;
        };
        # Mainly for building everything in CI.
        all = pkgs.buildEnv {
          name = "rose-all";
          paths = [
            rose-zig
            rose-py
          ];
        };
      };
    });
}
