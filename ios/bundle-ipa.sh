#!/bin/sh
# Builds Poopterm for iPhone and iPad devices as an IPA, ready to sign and
# install (a sideloading tool, or Xcode with your team, signs it):
#
#   ios/bundle-ipa.sh            # target/ios/Poopterm.ipa
#
# The app carries its bundled command frameworks (ios/commands) and every
# package of ios/packages, so they work from the first launch, signed with the app;
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
mkdir -p "$app/Frameworks" "$app/licenses" "$out/icon"
"$here/terminfo.sh" "$app/terminfo" >/dev/null
cp target/aarch64-apple-ios/release/poopterm "$app/Poopterm"
embed_command() {
	command=$1
	name=$(basename "$command")
	framework="$app/Frameworks/$name.framework"
	mkdir -p "$framework"
	cp "$command" "$framework/$name"
	cat >"$framework/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
	<key>CFBundleExecutable</key><string>$name</string>
	<key>CFBundleIdentifier</key><string>labs.poopterm.command.$name</string>
	<key>CFBundleName</key><string>$name</string>
	<key>CFBundlePackageType</key><string>FMWK</string>
	<key>CFBundleVersion</key><string>1</string>
	<key>MinimumOSVersion</key><string>17.0</string>
</dict></plist>
PLIST
}
for command in target/ios-commands/iphoneos/*; do
	embed_command "$command"
done
# Each package's commands are framework bundles for iOS code signing and
# sideloader re-signing; its license is kept apart.
# Bundled ports that are not Apple's command projects bring their own
# licenses: the One True AWK, less and GNU diffutils.
ports=target/ios-ports
for license in awk:awk/src/LICENSE less:less/less/LICENSE diffutils:gnudiff/diffutils/COPYING; do
	mkdir -p "$app/licenses/${license%%:*}"
	cp "$ports/${license#*:}" "$app/licenses/${license%%:*}/"
done
for package in target/ios-packages/work-iphoneos/*/; do
	# Not `name`: embed_command sets that for each command, and a package
	# with several (ssh, dropbearkey) would lose its own.
	package_name=$(basename "$package")
	for command in "$package"bin/*; do
		embed_command "$command"
	done
	# tcc's headers and runtime sources: the app's own tcc folder.
	if [ -d "$package/share/tcc" ]; then
		rm -rf "$app/tcc"
		cp -R "$package/share/tcc" "$app/tcc"
	fi
	if [ -d "$package/share/doc/$package_name" ]; then
		mkdir -p "$app/licenses/$package_name"
		cp -R "$package/share/doc/$package_name/." "$app/licenses/$package_name/"
	fi
done

# The icon, and the Info.plist keys that name it.
xcrun actool assets/icon.icon --compile "$app" --platform iphoneos \
	--minimum-deployment-target 17.0 --app-icon icon --target-device iphone \
	--target-device ipad --output-partial-info-plist "$out/icon/partial.plist" >/dev/null
python3 -I "$here/merge_plist.py" "$here/Info.plist" "$out/icon/partial.plist" "$app/Info.plist"

for framework in "$app"/Frameworks/*.framework; do
	codesign --force --sign - "$framework" 2>/dev/null
done
codesign --force --sign - --entitlements "$here/Poopterm.entitlements" "$app" 2>/dev/null
(cd "$out" && COPYFILE_DISABLE=1 zip -qr -X Poopterm.ipa Payload)
echo "built $out/Poopterm.ipa: $(find "$app/Frameworks" -name '*.framework' -type d | wc -l | tr -d ' ') command frameworks, $(du -h "$out/Poopterm.ipa" | cut -f1)"
