#!/usr/bin/env bash
# Build an installable Debian package from the release binary.
set -euo pipefail
cd "$(dirname "$0")/.."

binary=${1:-target/release/poopterm}
if [[ ! -x "$binary" ]]; then
	echo "package-deb.sh: executable not found: $binary" >&2
	exit 1
fi
command -v dpkg-deb >/dev/null
command -v dpkg-shlibdeps >/dev/null
command -v tic >/dev/null

version=$("$binary" --version | sed -n 's/^poopterm //p')
if [[ -z "$version" ]]; then
	echo 'package-deb.sh: could not read version from the release binary' >&2
	exit 1
fi
debian_version="${version}-1"
architecture=$(dpkg --print-architecture)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
root="$work/root"

DESTDIR="$root" PREFIX=/usr POOPTERM_BIN="$binary" scripts/install.sh --no-build
mkdir -p "$root/DEBIAN" "$work/debian" dist
rm -f dist/poopterm_*.deb

cat >"$work/debian/control" <<EOF
Source: poopterm
Section: x11
Priority: optional
Maintainer: Poopterm Labs <poopterm-labs@users.noreply.github.com>
Homepage: https://github.com/poopware-laboratories/poopterm

Package: poopterm
Architecture: $architecture
Depends: \${shlibs:Depends}
Description: fast software-rendered terminal emulator
 Poopterm is a tabbed terminal emulator with a software-rendered interface,
 configurable themes, image protocol support, and shell integration.
EOF

depends=$(cd "$work" && dpkg-shlibdeps -O -e "$root/usr/bin/poopterm" | sed -n 's/^shlibs:Depends=//p')
if [[ -z "$depends" ]]; then
	echo 'package-deb.sh: could not determine shared-library dependencies' >&2
	exit 1
fi

cat >"$root/DEBIAN/control" <<EOF
Package: poopterm
Version: $debian_version
Section: x11
Priority: optional
Architecture: $architecture
Depends: $depends
Maintainer: Poopterm Labs <poopterm-labs@users.noreply.github.com>
Homepage: https://github.com/poopware-laboratories/poopterm
Description: fast software-rendered terminal emulator
 Poopterm is a tabbed terminal emulator with a software-rendered interface,
 configurable themes, image protocol support, and shell integration.
EOF

install -Dm644 LICENSE "$root/usr/share/doc/poopterm/copyright"
install -Dm644 THIRD_PARTY_NOTICES "$root/usr/share/doc/poopterm/THIRD_PARTY_NOTICES"
dpkg-deb --root-owner-group --build "$root" "dist/poopterm_${debian_version}_${architecture}.deb"
