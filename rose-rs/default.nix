{ rustPlatform
, version
}:

rustPlatform.buildRustPackage {
  pname = "rose-rs";
  inherit version;
  src = ./.;
  cargoLock = { lockFile = ./Cargo.lock; };
  doCheck = false;
}
