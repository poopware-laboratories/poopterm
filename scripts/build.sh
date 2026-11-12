#!/bin/sh
# Local macOS builds tune code generation to the CPU running rustc. Packages,
# CI and cross builds keep the portable level: the x86-64-v2 features on
# x86-64, which .cargo/config.toml sets unless RUSTFLAGS replaces it, so they
# are added here whenever RUSTFLAGS is set. Explicit CPU flags win.
# Usage: sh scripts/build.sh --release --locked (arguments go to cargo build).
set -eu

host=$(rustc -vV | sed -n 's/^host: //p')
target=${CARGO_BUILD_TARGET:-$host}
next=0
targets=0
for argument in "$@"; do
	if [ "$next" = 1 ]; then
		target=$argument; next=0; targets=$((targets + 1))
		continue
	fi
	case $argument in
	--target) next=1 ;;
	--target=*) target=${argument#--target=}; targets=$((targets + 1)) ;;
	esac
done
# Several targets, or a --target without its value: no one CPU fits.
[ "$targets" -gt 1 ] || [ "$next" = 1 ] && target=

level=
if [ "$(uname -s)" = Darwin ] && [ -z "${CI:-}" ] && [ -z "${DESTDIR:-}" ] &&
	[ "${POOPTERM_PORTABLE:-0}" != 1 ] && [ -n "$host" ] && [ "$target" = "$host" ]; then
	level=-Ctarget-cpu=native
elif [ "${RUSTFLAGS+x}${CARGO_ENCODED_RUSTFLAGS+x}" ]; then
	case $target in
	x86_64-*) level=-Ctarget-feature=+sse3,+ssse3,+sse4.1,+sse4.2,+popcnt,+cmpxchg16b ;;
	esac
fi

if [ -n "$level" ]; then
	if [ "${CARGO_ENCODED_RUSTFLAGS+x}" = x ]; then
		case $CARGO_ENCODED_RUSTFLAGS in
		*target-cpu*) ;;
		*)
			separator=$(printf '\037')
			CARGO_ENCODED_RUSTFLAGS="${CARGO_ENCODED_RUSTFLAGS:+$CARGO_ENCODED_RUSTFLAGS$separator}$level"
			export CARGO_ENCODED_RUSTFLAGS
			;;
		esac
	else
		case ${RUSTFLAGS:-} in
		*target-cpu*) ;;
		*) RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }$level"; export RUSTFLAGS ;;
		esac
	fi
fi
exec cargo build "$@"
