# generated by zon2nix (https://github.com/Cloudef/zig2nix)

{ lib, linkFarm, fetchurl, fetchgit, runCommandLocal, zig, name ? "zig-packages" }:

with builtins;
with lib;

let
  unpackZigArtifact = { name, artifact }: runCommandLocal name {
      nativeBuildInputs = [ zig ];
    } ''
      hash="$(zig fetch --global-cache-dir "$TMPDIR" ${artifact})"
      mv "$TMPDIR/p/$hash" "$out"
      chmod 755 "$out"
    '';

  fetchZig = { name, url, hash }: let
    artifact = fetchurl { inherit url hash; };
  in unpackZigArtifact { inherit name artifact; };

  fetchGitZig = { name, url, hash }: let
    parts = splitString "#" url;
    base = elemAt parts 0;
    rev = elemAt parts 1;
  in fetchgit {
    inherit name rev hash;
    url = base;
    deepClone = false;
  };

  fetchZigArtifact = { name, url, hash }: let
    parts = splitString "://" url;
    proto = elemAt parts 0;
    path = elemAt parts 1;
    fetcher = {
      "git+http" = fetchGitZig { inherit name hash; url = "http://${path}"; };
      "git+https" = fetchGitZig { inherit name hash; url = "https://${path}"; };
      http = fetchZig { inherit name hash; url = "http://${path}"; };
      https = fetchZig { inherit name hash; url = "https://${path}"; };
      file = unpackZigArtifact { inherit name; artifact = /. + path; };
    };
  in fetcher.${proto};
in linkFarm name [
  {
    name = "1220e0961c135c5aa3af77a043dbc5890a18235a157238df0e2882fe84a8c8439c7a";
    path = fetchZigArtifact {
      name = "sqlite";
      url = "https://github.com/vrischmann/zig-sqlite/archive/dc339b7cf3bca82a12c2169231dd247587766781.tar.gz";
      hash = "sha256-PbWUedWBuNxA7diVAEh225b6YQA757aSvomQjUgSgRk=";
    };
  }
]