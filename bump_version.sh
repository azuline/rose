#!/usr/bin/env bash

version="$1"
message="$2"

[ -z "$version" ] && echo "Must specify a version." && exit 1
[ -z "$message" ] && echo "Must specify a message." && exit 1

# Fail if the branch is dirty.
git diff --exit-code

echo "$version" > "$ROSE_ROOT/rose/.version"
git add .
git commit -am "[Release] v$version - $message"
git tag "$version" HEAD -m "v$version"
git push origin "v$version"
git checkout release
git reset --hard "v$version"
git push --force
