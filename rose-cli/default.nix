{ python-pin
, version
, py-deps
, rose-py
, rose-vfs
, rose-watch
}:

python-pin.pkgs.buildPythonApplication {
  pname = "rose";
  version = version;
  src = ./.;
  propagatedBuildInputs = [
    rose-py
    rose-vfs
    rose-watch
    py-deps.click
  ];
  doCheck = false;
}
