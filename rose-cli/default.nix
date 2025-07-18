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
  pyproject = true;
  build-system = [ py-deps.setuptools ];
  propagatedBuildInputs = [
    rose-py
    rose-vfs
    rose-watch
    py-deps.click
  ];
  doCheck = false;
}
