#!/bin/sh
# Copies the terminal descriptions Poopterm's programs need (less, stty,
# editors) from the build machine's ncurses database into a directory the
# app carries, since an iPhone has no /usr/share/terminfo. The app points
# TERMINFO at it. Each entry is there twice: under its first letter, where
# ncurses built from source (nano) looks, and under that letter's two-digit
# hex code, where Apple's own libncurses (less, ls colors) looks.
#
#   ios/terminfo.sh <output directory>
set -eu

out=$1
source=${TERMINFO_SOURCE:-/usr/share/terminfo}
rm -rf "$out"
mkdir -p "$out"
for name in xterm-256color xterm xterm-color xterm-new xterm-16color vt100 vt220 \
	screen screen-256color tmux tmux-256color linux ansi dumb; do
	first=$(printf '%s' "$name" | head -c1 | od -An -tx1 | tr -d ' \n')
	file="$source/$first/$name"
	[ -e "$file" ] || { echo "missing terminfo entry $name in $source" >&2; exit 1; }
	letter=$(printf '%s' "$name" | head -c1)
	mkdir -p "$out/$first" "$out/$letter"
	cp -L "$file" "$out/$first/$name"
	cp -L "$file" "$out/$letter/$name"
done
echo "terminfo: $(find "$out" -type f | wc -l | tr -d ' ') files in $out"
