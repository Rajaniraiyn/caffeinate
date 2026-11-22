#!/usr/bin/env bash
set -euo pipefail

binary=$1
version=$2
license=$3
out=$4
work=$(mktemp -d)
trap 'rm -rf -- "$work"' EXIT

install -Dm755 "$binary" "$work/deb/usr/bin/caffeinate"
mkdir -p "$work/deb/usr/share/doc/caffeinate"
{
  printf 'caffeinate is licensed under MIT or Apache-2.0.\n\n'
  cat LICENSE-MIT LICENSE-APACHE "$license"
} > "$work/deb/usr/share/doc/caffeinate/copyright"
mkdir "$work/deb/DEBIAN"
cat > "$work/deb/DEBIAN/control" <<EOF
Package: caffeinate
Version: $version
Architecture: $(dpkg --print-architecture)
Maintainer: Rajaniraiyn <72294760+Rajaniraiyn@users.noreply.github.com>
Section: utils
Priority: optional
Description: Keep your computer awake from the command line
EOF
dpkg-deb --root-owner-group --build "$work/deb" "$out"

rpmbuild -bb .github/packaging/caffeinate.spec \
  --define "_topdir $work/rpm" \
  --define "_rpmdir $out" \
  --define '_rpmfilename %{NAME}-%{VERSION}-%{RELEASE}.%{ARCH}.rpm' \
  --define "package_version $version" \
  --define "binary $binary" \
  --define "project_dir $PWD" \
  --define "dbus_license $license"
