{ callPackage
, stdenv
, zig
, version
}:

stdenv.mkDerivation {
  pname = "rose-zig";
  version = version;
  src = ./.;
  nativeBuildInputs = [ zig.hook ];
  postPatch = ''
    ln -s ${callPackage ./deps.nix { }} $ZIG_GLOBAL_CACHE_DIR/p
  '';
}
