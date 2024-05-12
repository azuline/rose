{ python-pin
, version
, py-deps
, rose-py
, rose-vfs
, rose-watchdog
}:

python-pin.pkgs.buildPythonPackage {
  pname = "rose-cli";
  version = version;
  src = ./.;
  propagatedBuildInputs = [
    rose-py
    rose-vfs
    rose-watchdog
    py-deps.click
  ];
  doCheck = false;
}
