{ stdenv
, zig
, version
}:

stdenv.mkDerivation {
  pname = "rose-zig";
  version = version;
  src = ./.;
  nativeBuildInputs = [ zig.hook ];
}
