{ python-pin
, version
, py-deps
, music-py
}:

python-pin.pkgs.buildPythonPackage {
  pname = "music-watch";
  version = version;
  src = ./.;
  pyproject = true;
  build-system = [ py-deps.setuptools ];
  propagatedBuildInputs = [
    music-py
    py-deps.watchdog
  ];
  doCheck = false;
}
