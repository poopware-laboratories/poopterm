# Builds and installs system-wide through scripts/install.sh; the pgo
# targets build a profile-guided optimized binary through scripts/pgo.sh,
# and on Linux bolt lays it out again with llvm-bolt.
#
#   make                 release build
#   make install         release build, installed to $(PREFIX)
#   make pgo             instrument, train on the benchmark, build optimized
#   make pgo-use         run the instrumented terminal; your session trains it
#   make pgo-build       rebuild optimized from every profile so far
#   make bolt            Linux: profile the optimized build and bolt it
#   make pgo-install     build what releases ship (PGO, plus BOLT on Linux)
#                        and install it
#   make pgo-compare     benchmark the release, PGO and BOLT builds
#   make pgo-clean       forget all profiles
PREFIX ?= /usr/local
DESTDIR ?=
# MiB per benchmark workload when training.
PGO_MIB ?= 8
POOPTERM_PORTABLE ?= 0

all:
	sh scripts/build.sh --release --locked

run: all
	./target/release/poopterm

install: all
	PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) sh scripts/install.sh --no-build

uninstall:
	PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) sh scripts/install.sh --uninstall

pgo:
	sh scripts/pgo.sh all $(PGO_MIB)

pgo-use:
	sh scripts/pgo.sh use

pgo-build:
	sh scripts/pgo.sh build

bolt:
	sh scripts/pgo.sh bolt $(PGO_MIB)

pgo-install:
	POOPTERM_PORTABLE=$(POOPTERM_PORTABLE) sh scripts/pgo.sh dist $(PGO_MIB)
	PREFIX=$(PREFIX) DESTDIR=$(DESTDIR) sh scripts/install.sh --no-build

pgo-compare:
	sh scripts/pgo.sh compare

pgo-clean:
	sh scripts/pgo.sh clean

clean:
	cargo clean

.PHONY: all run install uninstall pgo pgo-use pgo-build bolt pgo-install pgo-compare pgo-clean clean
