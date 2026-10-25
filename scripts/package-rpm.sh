#!/usr/bin/env bash
# Build an installable RPM package (Fedora and relatives) from the release binary.
set -euo pipefail
cd "$(dirname "$0")/.."

binary=${1:-target/release/poopterm}
if [[ ! -x "$binary" ]]; then
	echo "package-rpm.sh: executable not found: $binary" >&2
	exit 1
fi
command -v rpmbuild >/dev/null
command -v tic >/dev/null

version=$("$binary" --version | sed -n 's/^poopterm //p')
if [[ -z "$version" ]]; then
	echo 'package-rpm.sh: could not read version from the release binary' >&2
	exit 1
fi
architecture=$(uname -m)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
root="$work/root"

DESTDIR="$root" PREFIX=/usr POOPTERM_BIN="$binary" scripts/install.sh --no-build
install -Dm644 LICENSE "$root/usr/share/licenses/poopterm/LICENSE"
install -Dm644 THIRD_PARTY_NOTICES "$root/usr/share/doc/poopterm/THIRD_PARTY_NOTICES"
mkdir -p dist
rm -f dist/poopterm-*.rpm

# Files only, never directories: /usr/bin and the rest belong to filesystem.
files=$(cd "$root" && find . ! -type d | sed -e 's|^\.||' \
	-e 's|^/usr/share/licenses/|%license &|' -e 's|^/usr/share/doc/|%doc &|' | sort)

cat >"$work/poopterm.spec" <<SPEC
Name: poopterm
Version: $version
Release: 1
Summary: Fast software-rendered terminal emulator
License: AGPL-3.0-or-later
URL: https://github.com/poopware-laboratories/poopterm
# winit loads the display libraries at run time, so rpm's ELF scan cannot
# see them; xkbcommon serves both Wayland and X11.
Requires: libxkbcommon
Recommends: libwayland-client libwayland-cursor libxkbcommon-x11 libX11 libX11-xcb libXcursor libXi libXrandr

# Releases ship a PGO and BOLT build: no stripping, debuginfo or build-id links.
%global debug_package %{nil}
%global __os_install_post %{nil}
%global _build_id_links none

%description
Poopterm is a tabbed terminal emulator with a software-rendered interface,
configurable themes, image protocol support, and shell integration.

%install
cp -a "$root/." %{buildroot}/

%files
$files
SPEC

rpmbuild -bb --quiet --target "$architecture" --define "_topdir $work/rpmbuild" \
	--define "_rpmdir $PWD/dist" --define '_rpmfilename %%{NAME}-%%{VERSION}-%%{RELEASE}.%%{ARCH}.rpm' \
	"$work/poopterm.spec"
