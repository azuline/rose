{ stdenv
, zig
, version
, rose-zig
}:

stdenv.mkDerivation {
  pname = "rose-ffi";
  version = version;
  src = ./.;
  nativeBuildInputs = [ zig.hook rose-zig ];
}
