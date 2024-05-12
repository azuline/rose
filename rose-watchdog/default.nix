{ python-pin
, version
, py-deps
, rose-py
}:

python-pin.pkgs.buildPythonPackage {
  pname = "rose-watchdog";
  version = version;
  src = ./.;
  propagatedBuildInputs = [
    rose-py
    py-deps.watchdog
  ];
  doCheck = false;
}
