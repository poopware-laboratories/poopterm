#!/bin/sh
# Profile-guided optimization: builds poopterm instrumented, trains it, and
# rebuilds it optimized for what the training ran. On Linux, BOLT then lays
# that build out again from a profile of its own.
#
#   scripts/pgo.sh                 # instrument, train on the benchmark, build
#   scripts/pgo.sh instrument      # instrumented build only
#   scripts/pgo.sh train [MIB]     # run the benchmark corpus (default 8 MiB each),
#                                  # as floods and as small interactive writes
#   scripts/pgo.sh use             # open the instrumented terminal and use it;
#                                  # closing it saves what you did as training
#   scripts/pgo.sh build           # merge every profile and build optimized
#   scripts/pgo.sh bolt [MIB]      # Linux: instrument the PGO build with
#                                  # llvm-bolt, train it, and lay it out again
#   scripts/pgo.sh compare [MIB]   # benchmark the release, PGO and BOLT builds
#   scripts/pgo.sh path            # print the optimized binary's path
#   scripts/pgo.sh dist [MIB]      # instrument, train and build, BOLT where it
#                                  # can, then put the result at
#                                  # target/release/poopterm; if PGO fails, a
#                                  # plain release build, if BOLT fails, PGO's
#   scripts/pgo.sh clean           # forget all profiles
#
# Profiles add up: train on the benchmark, add sessions with `use`, then
# `build` again. Old profiles still apply after code changes (functions that
# changed are left unoptimized), but retrain after large ones.
#
# LLVM_BOLT and MERGE_FDATA name the BOLT tools when they carry a version
# suffix, as Debian's llvm-bolt-18 does.
set -eu

root=$(cd "$(dirname "$0")/.." && pwd)
cd "$root"
host=$(rustc -vV | sed -n 's/^host: //p')
pgo=$root/target/pgo
profiles=$pgo/profiles
merged=$pgo/merged.profdata
# Separate target directories, so the instrumented and optimized builds do
# not rebuild each other, nor the plain release build.
instrumented=$pgo/instrumented/$host/release/poopterm
optimized=$pgo/optimized/$host/release/poopterm
bolt_dir=$pgo/bolt
bolted=$bolt_dir/poopterm
llvm_bolt=${LLVM_BOLT:-llvm-bolt}
merge_fdata=${MERGE_FDATA:-merge-fdata}
# BOLT rewrites Linux ELF binaries only.
linux() {
	[ "$(uname -s)" = Linux ]
}

# llvm-profdata must match rustc's LLVM, so use the toolchain's own.
profdata() {
	tool=$(rustc --print sysroot)/lib/rustlib/$host/bin/llvm-profdata
	if [ ! -x "$tool" ]; then
		echo "pgo.sh: adding the llvm-tools component for llvm-profdata" >&2
		rustup component add llvm-tools >&2
	fi
	echo "$tool"
}

# Builds for the host target explicitly, so the flags reach poopterm and
# its dependencies but not build scripts. Existing RUSTFLAGS are kept.
cargo_build() {
	target_dir=$1
	shift
	RUSTFLAGS="${RUSTFLAGS:-} $*" sh scripts/build.sh --release --locked --target "$host" \
		--target-dir "$target_dir"
}

instrument() {
	mkdir -p "$profiles"
	cargo_build "$pgo/instrumented" "-Cprofile-generate=$profiles"
}

need_instrumented() {
	[ -x "$instrumented" ] || instrument
}

# Runs the training corpus on the instrumented binary $1.
run_training() {
	"$1" --benchmark --mib "${2:-8}"
	# Interactive programs write a few bytes at a time (key echo, prompts)
	# and get a frame for most writes; keep their paths in the profile too.
	"$1" --benchmark --kib 64 --read 16 --frame-every 64
}

train() {
	need_instrumented
	run_training "$instrumented" "$@"
}

# The binary releases ship, at the path packaging expects. Training runs
# in its own process, so a failure anywhere in it falls back.
dist() {
	# Distribution builds target the architecture baseline, unless a local
	# make pgo-install explicitly requests this machine's CPU. CI stays portable.
	export POOPTERM_PORTABLE=${POOPTERM_PORTABLE:-1}
	release=$root/target/release/poopterm
	if ! sh "$root/scripts/pgo.sh" all "$@"; then
		echo "::warning::pgo.sh: PGO failed; shipping a plain release build" >&2
		sh scripts/build.sh --release --locked
		return
	fi
	mkdir -p "$(dirname "$release")"
	if ! linux; then
		cp "$optimized" "$release"
		echo "pgo.sh: shipping the PGO build"
		return
	fi
	for tool in "$llvm_bolt" "$merge_fdata"; do
		if ! command -v "$tool" >/dev/null; then
			echo "::notice::pgo.sh: no $tool; shipping the PGO build" >&2
			strip -o "$release" "$optimized"
			return
		fi
	done
	if sh "$root/scripts/pgo.sh" bolt "$@"; then
		cp "$bolted" "$release"
		echo "pgo.sh: shipping the PGO+BOLT build"
	else
		echo "::warning::pgo.sh: BOLT failed; shipping the PGO build" >&2
		strip -o "$release" "$optimized"
	fi
}

use_terminal() {
	need_instrumented
	echo "Use this terminal as usual; its profile is saved when you close it."
	"$instrumented"
}

build() {
	if ! ls "$profiles"/*.profraw >/dev/null 2>&1; then
		echo "pgo.sh: no profiles yet; run 'scripts/pgo.sh train' first" >&2
		exit 1
	fi
	"$(profdata)" merge -o "$merged" "$profiles"/*.profraw
	if linux; then
		# BOLT needs the relocations to move code, and the symbols; `bolt`
		# and `dist` strip what they ship.
		export CARGO_PROFILE_RELEASE_STRIP=false
		cargo_build "$pgo/optimized" -Clink-arg=-Wl,--emit-relocs "-Cprofile-use=$merged"
	else
		cargo_build "$pgo/optimized" "-Cprofile-use=$merged"
	fi
	echo "Optimized build: $optimized"
}

# Lays the PGO build out again for the code its training runs: hot blocks
# and functions together, cold ones split off. Each run starts afresh.
bolt() {
	[ -x "$optimized" ] || build
	rm -rf "$bolt_dir"
	mkdir -p "$bolt_dir"
	"$llvm_bolt" "$optimized" -o "$bolt_dir/instrumented" -instrument \
		-instrumentation-file="$bolt_dir/profile" -instrumentation-file-append-pid
	run_training "$bolt_dir/instrumented" "$@"
	"$merge_fdata" "$bolt_dir"/profile.*.fdata >"$bolt_dir/merged.fdata"
	# Without huge-page alignment the new code fits the old text, so the
	# binary keeps its size.
	"$llvm_bolt" "$optimized" -o "$bolted" -data="$bolt_dir/merged.fdata" \
		-reorder-blocks=ext-tsp -reorder-functions=cdsort -split-functions -split-all-cold \
		-split-eh -icf -use-gnu-stack -no-huge-pages -use-old-text
	strip "$bolted"
	echo "Bolted build: $bolted"
}

compare() {
	[ -x "$optimized" ] || {
		echo "pgo.sh: no optimized build; run 'scripts/pgo.sh build' first" >&2
		exit 1
	}
	sh scripts/build.sh --release --locked
	for binary in "$root/target/release/poopterm" "$optimized" "$bolted"; do
		[ -x "$binary" ] || continue
		echo "== $binary"
		"$binary" --benchmark --mib "${1:-16}"
	done
}

command=${1:-all}
[ $# -gt 0 ] && shift
case $command in
all)
	instrument
	train "$@"
	build
	;;
instrument) instrument ;;
train) train "$@" ;;
use) use_terminal ;;
build) build ;;
bolt) bolt "$@" ;;
compare) compare "$@" ;;
path) echo "$optimized" ;;
dist) dist "$@" ;;
clean) rm -rf "$profiles" "$merged" "$bolt_dir" ;;
-h | --help) sed -n '2,28p' "$0" | sed 's/^# \{0,1\}//' ;;
*)
	echo "pgo.sh: unknown command: $command (try --help)" >&2
	exit 2
	;;
esac
