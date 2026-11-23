#!/usr/bin/env bash
# Unpack the gems oracle/intake.json reads into an OpenProject checkout that has
# no vendor/bundle (CI checks out the source only).
#
# The gem set is every `gems/<name>-<version>/` path the intake names, so the
# intake stays the one list. Each .gem is downloaded from rubygems.org and
# checked against the sha256 the checkout's own Gemfile.lock records in its
# CHECKSUMS section before it is unpacked; a gem the lock has no checksum for
# is an error. The files land in vendor/bundle/ruby/ci/gems/<name>-<version>/,
# which the intake's `vendor/bundle/ruby/*/gems/...` patterns match. Unpacking
# a .gem's data.tar.gz gives the same tree `bundle install` does (checked
# against a bundled checkout for the five gems the intake names at
# OpenProject 2725194: no file differs).
#
# usage: scripts/fetch-oracle-gems.sh <openproject checkout> [intake.json]
set -euo pipefail

src=${1:?usage: fetch-oracle-gems.sh <openproject checkout> [intake.json]}
intake=${2:-$(dirname "$0")/../oracle/intake.json}
lock="$src/Gemfile.lock"
dest="$src/vendor/bundle/ruby/ci/gems"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

gems=$(grep -o 'gems/[A-Za-z0-9_.-]*-[0-9][0-9.]*/' "$intake" | sed 's#^gems/##; s#/$##' | sort -u)
[ -n "$gems" ] || { echo "no gem paths in $intake" >&2; exit 1; }

mkdir -p "$dest"
for gv in $gems; do
  name=${gv%-*}
  version=${gv##*-}
  want=$(grep -E "^  ${name} \(${version}\) sha256=" "$lock" | sed 's/.*sha256=//' || true)
  if [ -z "$want" ]; then
    echo "$lock has no checksum for $name $version" >&2
    exit 1
  fi
  curl -sSLf --retry 3 -o "$work/$gv.gem" "https://rubygems.org/downloads/$gv.gem"
  got=$(sha256sum "$work/$gv.gem" | cut -d' ' -f1)
  if [ "$got" != "$want" ]; then
    echo "$gv.gem: sha256 $got, Gemfile.lock says $want" >&2
    exit 1
  fi
  rm -rf "${dest:?}/$gv"
  mkdir -p "$dest/$gv"
  tar -xOf "$work/$gv.gem" data.tar.gz | tar -xz -C "$dest/$gv"
  echo "unpacked $gv (sha256 $got)"
done
