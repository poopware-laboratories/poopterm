#!/bin/sh
# Builds Poopterm for iPhone and iPad devices as an IPA, ready to sign and
# install (a sideloading tool, or Xcode with your team, signs it):
#
#   ios/bundle-ipa.sh            # target/ios/Poopterm.ipa
#
# The app carries its bundled commands (ios/commands) and every package of
# ios/packages, so they work from the first launch, signed with the app;
# packages installed later with `pkg` need the JIT a debugger gives. Its
# icon is assets/icon.icon, compiled by actool, light and dark; each port's
# license sits in Poopterm.app/licenses/<package>.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
cd "$root"
out=target/ios
app="$out/Payload/Poopterm.app"

# The app's minimum iOS, for Rust and the C its crates compile (mimalloc's
# optimised code needs what older targets lack).
export IPHONEOS_DEPLOYMENT_TARGET=17.0
cargo build --release --target aarch64-apple-ios
"$here/commands/build.sh" iphoneos
"$here/packages/build.sh" iphoneos

rm -rf "$out/Payload" "$out/Poopterm.ipa" "$out/icon"
mkdir -p "$app/commands" "$app/licenses" "$out/icon"
cp target/aarch64-apple-ios/release/poopterm "$app/Poopterm"
cp target/ios-commands/iphoneos/* "$app/commands/"
# Each package's commands beside the bundled ones, its license apart.
for package in target/ios-packages/work-iphoneos/*/; do
	name=$(basename "$package")
	cp "$package"bin/* "$app/commands/"
	if [ -d "$package/share/doc/$name" ]; then
		mkdir -p "$app/licenses/$name"
		cp -R "$package/share/doc/$name/." "$app/licenses/$name/"
	fi
done

# The icon, and the Info.plist keys that name it.
xcrun actool assets/icon.icon --compile "$app" --platform iphoneos \
	--minimum-deployment-target 17.0 --app-icon icon --target-device iphone \
	--target-device ipad --output-partial-info-plist "$out/icon/partial.plist" >/dev/null
python3 -I "$here/merge_plist.py" "$here/Info.plist" "$out/icon/partial.plist" "$app/Info.plist"

for command in "$app"/commands/*; do
	codesign --force --sign - "$command" 2>/dev/null
done
codesign --force --sign - "$app" 2>/dev/null
(cd "$out" && COPYFILE_DISABLE=1 zip -qr -X Poopterm.ipa Payload)
echo "built $out/Poopterm.ipa: $(ls "$app/commands" | wc -l | tr -d ' ') commands, $(du -h "$out/Poopterm.ipa" | cut -f1)"
