{ python-pin
, version
, py-deps
, music-py
, music-vfs
, music-watch
}:

python-pin.pkgs.buildPythonApplication {
  pname = "music";
  version = version;
  src = ./.;
  pyproject = true;
  build-system = [ py-deps.setuptools ];
  propagatedBuildInputs = [
    music-py
    music-vfs
    music-watch
    py-deps.click
  ];
  doCheck = false;
}
