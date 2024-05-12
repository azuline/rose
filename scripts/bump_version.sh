#!/usr/bin/env bash

version="$1"
message="$2"

[ -z "$version" ] && echo "Must specify a version." && exit 1
[ -z "$message" ] && echo "Must specify a message." && exit 1

# Fail if the branch is dirty.
git diff --exit-code

fd -u '^.version$' | xargs -I{} sh -c "echo $version > "'"$1"' -- {}
git add .
git commit -am "[Release] v$version - $message"
git tag "v$version" HEAD -m "v$version"
git push origin master
git push origin "v$version"
git checkout release
git reset --hard "v$version"
git push --force
