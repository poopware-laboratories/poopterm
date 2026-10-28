#!/bin/sh
# Installs poopterm system-wide, so every user, desktop, and sudo session
# finds it: binary, desktop entry, hicolor icons, and the terminfo entry.
#
#   scripts/install.sh               # builds as you, installs to /usr/local
#   scripts/install.sh --uninstall
#
# Run it as yourself: it asks for root (sudo or doas) only to copy files.
# PREFIX (default /usr/local) and DESTDIR (staging, for packaging) are
# honored. --no-build skips `cargo build`; POOPTERM_BIN picks the binary.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
action=install
build=1
for argument in "$@"; do
	case $argument in
	--uninstall) action=uninstall ;;
	--no-build) build=0 ;;
	-h | --help)
		sed -n '2,10p' "$0" | sed 's/^# \{0,1\}//'
		exit 0
		;;
	*)
		echo "install.sh: unknown option: $argument" >&2
		exit 2
		;;
	esac
done

PREFIX=${PREFIX:-/usr/local}
DESTDIR=${DESTDIR:-}
bindir=$PREFIX/bin
datadir=$PREFIX/share
icons="16x16:icon_16x16 32x32:icon_32x32 64x64:icon_32x32@2x 128x128:icon_128x128
256x256:icon_256x256 512x512:icon_512x512"

# Runs a command as root when the target is not writable as we are.
target=$DESTDIR$PREFIX
while [ ! -e "$target" ]; do target=$(dirname "$target"); done
as_root=
if [ ! -w "$target" ]; then
	for tool in sudo doas; do
		command -v "$tool" >/dev/null 2>&1 && as_root=$tool && break
	done
	[ -n "$as_root" ] || {
		echo "install.sh: $DESTDIR$PREFIX is not writable; run as root" >&2
		exit 1
	}
	echo "Installing to $DESTDIR$PREFIX needs root; asking $as_root."
fi
run() { $as_root "$@"; }

# Lets running desktops notice the change; harmless when the tools are missing.
refresh() {
	[ -z "$DESTDIR" ] || return 0
	command -v update-desktop-database >/dev/null 2>&1 &&
		run update-desktop-database -q "$datadir/applications" 2>/dev/null
	command -v gtk-update-icon-cache >/dev/null 2>&1 &&
		run gtk-update-icon-cache -q -t -f "$datadir/icons/hicolor" 2>/dev/null
	return 0
}

# ncurses reads entries only from its own search path, which may leave out
# $PREFIX/share/terminfo (Fedora searches /usr/share/terminfo and
# /etc/terminfo only). Use that directory when ncurses searches it, else the
# first system directory ncurses does. A prefix chosen on purpose is kept.
terminfo=$datadir/terminfo
if [ "$PREFIX" = /usr/local ] && command -v infocmp >/dev/null 2>&1; then
	searched=$(infocmp -D 2>/dev/null | grep -v "^$HOME/" || true)
	if [ -n "$searched" ] && ! printf '%s\n' "$searched" | grep -qx "$terminfo"; then
		terminfo=$(printf '%s\n' "$searched" | head -n 1)
	fi
fi

if [ "$action" = uninstall ]; then
	run rm -f "$DESTDIR$bindir/poopterm" "$DESTDIR$datadir/applications/poopterm.desktop" \
		"$DESTDIR$terminfo/x/xterm-poopterm" "$DESTDIR$terminfo/p/poopterm" \
		"$DESTDIR$terminfo/78/xterm-poopterm" "$DESTDIR$terminfo/70/poopterm"
	for icon in $icons; do
		run rm -f "$DESTDIR$datadir/icons/hicolor/${icon%%:*}/apps/poopterm.png"
	done
	refresh
	echo "Removed poopterm from $DESTDIR$PREFIX"
	exit 0
fi

binary=${POOPTERM_BIN:-$root/target/release/poopterm}
if [ "$build" = 1 ]; then
	command -v cargo >/dev/null 2>&1 || {
		echo "install.sh: cargo not found; run this script as yourself, not with sudo" >&2
		exit 1
	}
	(cd "$root" && sh scripts/build.sh --release --locked)
fi
[ -f "$binary" ] || {
	echo "install.sh: $binary not found; build it with cargo build --release" >&2
	exit 1
}

desktop=$(mktemp)
trap 'rm -f "$desktop"' EXIT
sed -e "s|^Exec=poopterm|Exec=$bindir/poopterm|" -e "s|^TryExec=poopterm|TryExec=$bindir/poopterm|" \
	"$root/packaging/poopterm.desktop" >"$desktop"

run install -d "$DESTDIR$bindir" "$DESTDIR$datadir/applications"
run install -m 755 "$binary" "$DESTDIR$bindir/poopterm"
run install -m 644 "$desktop" "$DESTDIR$datadir/applications/poopterm.desktop"
for icon in $icons; do
	run install -d "$DESTDIR$datadir/icons/hicolor/${icon%%:*}/apps"
	run install -m 644 "$root/assets/icon.iconset/${icon#*:}.png" \
		"$DESTDIR$datadir/icons/hicolor/${icon%%:*}/apps/poopterm.png"
done
# Shells get TERM=xterm-poopterm only with this entry; see src/terminfo.rs.
if command -v tic >/dev/null 2>&1; then
	run install -d "$DESTDIR$terminfo"
	run tic -x -o "$DESTDIR$terminfo" "$root/terminfo/poopterm.terminfo"
else
	echo "Note: tic not found; shells will use TERM=xterm-256color."
fi
refresh

echo "Installed poopterm to $DESTDIR$PREFIX"
