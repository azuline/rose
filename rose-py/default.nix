{ python-pin
, version
, py-deps
}:

python-pin.pkgs.buildPythonPackage {
  pname = "rose";
  version = version;
  src = ./.;
  pyproject = true;
  build-system = [ py-deps.setuptools ];
  propagatedBuildInputs = [
    py-deps.appdirs
    py-deps.click
    py-deps.jinja2
    py-deps.mutagen
    py-deps.llfuse
    py-deps.send2trash
    py-deps.tomli-w
    py-deps.uuid6
  ];
  doCheck = false;
}
