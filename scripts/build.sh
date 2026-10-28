#!/bin/sh
# Local macOS builds tune code generation to the CPU running rustc. Packages,
# CI and cross builds keep Cargo's portable defaults. Explicit CPU flags win.
# Usage: sh scripts/build.sh --release --locked (arguments go to cargo build).
set -eu

native=0
if [ "$(uname -s)" = Darwin ] && [ -z "${CI:-}" ] && [ -z "${DESTDIR:-}" ] &&
	[ "${POOPTERM_PORTABLE:-0}" != 1 ]; then
	host=$(rustc -vV | sed -n 's/^host: //p')
	target=${CARGO_BUILD_TARGET:-$host}
	next=0
	cross=0
	for argument in "$@"; do
		if [ "$next" = 1 ]; then
			target=$argument; next=0
			[ "$target" = "$host" ] || cross=1
			continue
		fi
		case $argument in
		--target) next=1 ;;
		--target=*)
			target=${argument#--target=}
			[ "$target" = "$host" ] || cross=1
			;;
		esac
	done
	[ -n "$host" ] && [ "$target" = "$host" ] && [ "$cross" = 0 ] && [ "$next" = 0 ] && native=1
fi

if [ "$native" = 1 ]; then
	if [ "${CARGO_ENCODED_RUSTFLAGS+x}" = x ]; then
		case $CARGO_ENCODED_RUSTFLAGS in
		*target-cpu*) ;;
		*)
			separator=$(printf '\037')
			CARGO_ENCODED_RUSTFLAGS="${CARGO_ENCODED_RUSTFLAGS:+$CARGO_ENCODED_RUSTFLAGS$separator}-Ctarget-cpu=native"
			export CARGO_ENCODED_RUSTFLAGS
			;;
		esac
	else
		case ${RUSTFLAGS:-} in
		*target-cpu*) ;;
		*) RUSTFLAGS="${RUSTFLAGS:+$RUSTFLAGS }-Ctarget-cpu=native"; export RUSTFLAGS ;;
		esac
	fi
fi
exec cargo build "$@"
