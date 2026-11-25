#!/bin/sh
# Stages the packages ios/packages/build.sh built for the `packages`
# release of Poopterm's repository, where `pkg` looks by default
# (src/pkg/source.rs). The release holds every platform's packages side by
# side, each file named after its platform: `ios-index` lists
# `ios-<name>-<version>.tar.gz`, and the Android port publishes
# `android-aarch64-index` and its archives to the same release.
#
#   ios/packages/release.sh [iphoneos|iphonesimulator|macosx]
#
# Output: target/packages-release/<platform>/, ready to upload with the
# command it prints. It uploads nothing itself.
set -eu

here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/../.." && pwd)
sdk=${1:-iphoneos}
case $sdk in
iphoneos) platform=ios ;;
iphonesimulator) platform=ios-simulator ;;
macosx) platform=macos ;;
*) echo "unknown SDK $sdk" >&2; exit 2 ;;
esac
built="$root/target/ios-packages/$sdk"
[ -f "$built/index" ] || { echo "no $built/index: run ios/packages/build.sh $sdk" >&2; exit 1; }
out="$root/target/packages-release/$platform"
rm -rf "$out"
mkdir -p "$out"
echo "# Poopterm packages for $platform" >"$out/$platform-index"
grep -v '^#' "$built/index" | while read -r name version hash size archive summary; do
	[ -n "$name" ] || continue
	# The base packages ship with the app; the release holds the others.
	grep -qx "$name" "$built/downloads" || continue
	cp "$built/$archive" "$out/$platform-$archive"
	echo "$name $version $hash $size $platform-$archive $summary" >>"$out/$platform-index"
done
echo "staged $(grep -vc '^#' "$out/$platform-index") packages in $out"
# The index last: a phone that fetches it finds every archive it lists.
repo="--clobber --repo poopware-laboratories/poopterm"
echo "upload: gh release upload packages $out/*.tar.gz $repo"
echo "  then: gh release upload packages $out/$platform-index $repo"
