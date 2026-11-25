#!/bin/sh
# Builds GNU make 4.4.1 (GPL-3.0-or-later) as a Poopterm command library:
# its sources as they come, with the hand-written config.h of this
# directory, on Darwin's own glob and fnmatch. Recipes start other
# commands through vfork and exec, which the command shim runs in the
# app, and /bin/sh -c lines, which the app's shell answers.
#
#   ios/packages/make/build.sh <iphonesimulator|iphoneos|macosx> <output>/bin/make
#
# The source is expected in target/ios-ports/make-4.4.1.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../../.." && pwd)
source="$root/target/ios-ports/make-4.4.1"
[ -d "$source" ] || { echo "missing $source: unpack make-4.4.1 there" >&2; exit 1; }
bin=$(dirname "$2")
# The command shim calls main with two arguments, so make reads its
# environment from `environ`, where the shell put it, not from main's third.
patched="$bin/../src"
mkdir -p "$patched"
python3 -I - "$source/src/main.c" "$patched/main.c" <<'PY'
import sys
text = open(sys.argv[1]).read()
old = "main (int argc, char **argv, char **envp)\n#endif\n{"
assert old in text
open(sys.argv[2], "w").write(text.replace(old, "main (int argc, char **argv)\n#endif\n{\n  extern char **environ;\n  char **envp = environ;", 1))
PY
files="$patched/main.c"
for name in ar arscan commands default dir expand file function getopt getopt1 guile hash implicit job load \
	loadapi misc output posixos read remake remote-stub rule shuffle signame strcache variable \
	version vpath; do
	files="$files $source/src/$name.c"
done
"$here/../cc.sh" "$1" "$2" -DHAVE_CONFIG_H -DLIBDIR='"/usr/lib"' -DLOCALEDIR='"/usr/share/locale"' \
	-DINCLUDEDIR='"/usr/include"' -I "$here" -I "$source/src" -I "$patched" -I "$source/lib" \
	$files
license="$bin/../share/doc/make"
mkdir -p "$license"
cp "$source/COPYING" "$license/COPYING"
echo "built $2"
