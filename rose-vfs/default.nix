{ python-pin
, version
, py-deps
, rose-py
}:

python-pin.pkgs.buildPythonPackage {
  pname = "rose-vfs";
  version = version;
  src = ./.;
  propagatedBuildInputs = [
    rose-py
    py-deps.llfuse
  ];
  doCheck = false;
}
