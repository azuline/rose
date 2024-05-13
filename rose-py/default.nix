{ python-pin
, version
, py-deps
}:

python-pin.pkgs.buildPythonPackage {
  pname = "rose";
  version = version;
  src = ./.;
  propagatedBuildInputs = [
    py-deps.appdirs
    py-deps.click
    py-deps.jinja2
    py-deps.mutagen
    py-deps.send2trash
    py-deps.tomli-w
    py-deps.uuid6
  ];
  doCheck = false;
}
