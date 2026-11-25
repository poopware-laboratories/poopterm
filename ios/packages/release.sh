#!/bin/sh
# Stages the packages ios/packages/build.sh built that the app does not
# carry, for `pkg` to find where it looks by default (src/pkg/source.rs):
# the archives for a release of their own in Poopterm's repository
# (releases there are immutable, so each upload is a new one), and the
# platform's index, naming each archive by its URL in that release, for
# the packages-index branch. The Android port publishes its own indexes
# (android-aarch64-index, ...) beside these.
#
#   ios/packages/release.sh [iphoneos|iphonesimulator|macosx] [tag]
#
# The tag defaults to packages-<platform>-<date and time>. Output:
# target/packages-release/<platform>/{archives,<platform>-index}, and the
# commands that publish them. It uploads nothing itself.
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
tag=${2:-packages-$platform-$(date -u +%Y%m%d-%H%M%S)}
repo=poopware-laboratories/poopterm
base="https://github.com/$repo/releases/download/$tag"
built="$root/target/ios-packages/$sdk"
[ -f "$built/index" ] || { echo "no $built/index: run ios/packages/build.sh $sdk" >&2; exit 1; }
out="$root/target/packages-release/$platform"
rm -rf "$out"
mkdir -p "$out/archives"
index="$out/$platform-index"
echo "# Poopterm packages for $platform, from the release $tag" >"$index"
grep -v '^#' "$built/index" | while read -r name version hash size archive summary; do
	[ -n "$name" ] || continue
	# The base packages ship with the app; the release holds the others.
	grep -qx "$name" "$built/downloads" || continue
	cp "$built/$archive" "$out/archives/$archive"
	echo "$name $version $hash $size $base/$archive $summary" >>"$index"
done
echo "staged $(grep -vc '^#' "$index") packages in $out for the release $tag"
cat <<EOF
publish:
  gh release create $tag --repo $repo --draft --prerelease --title "Packages for $platform" --notes "Packages for pkg on $platform. Not a Poopterm release."
  gh release upload $tag $out/archives/* --repo $repo
  gh release edit $tag --repo $repo --draft=false
then commit $index as $platform-index on the packages-index branch.
EOF
