#!/bin/sh
# Builds Poopterm for the iOS simulator as an app bundle, with the command
# frameworks (ios/commands/build.sh) under Poopterm.app/Frameworks, where the
# in-process shell finds them on $PATH:
#
#   ios/bundle-sim.sh && xcrun simctl install booted target/ios-sim/Poopterm.app
#
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
cd "$root"
cargo build --target aarch64-apple-ios-sim
"$here/commands/build.sh" iphonesimulator
app=target/ios-sim/Poopterm.app
rm -rf "$app"
mkdir -p "$app/Frameworks"
# The icon (assets/icon.icon), and the Info.plist keys that name it.
mkdir -p target/ios-sim/icon
xcrun actool assets/icon.icon --compile "$app" --platform iphonesimulator \
	--minimum-deployment-target 17.0 --app-icon icon --target-device iphone \
	--target-device ipad --output-partial-info-plist target/ios-sim/icon/partial.plist >/dev/null
python3 -I "$here/merge_plist.py" "$here/Info.plist" target/ios-sim/icon/partial.plist "$app/Info.plist"
# Built against the current SDK: the app adopts the scene lifecycle iOS 27
# requires (src/ios_scene.rs), so it resizes as a modern app.
cp target/aarch64-apple-ios-sim/debug/poopterm "$app/Poopterm"
chmod +x "$app/Poopterm"
for command in target/ios-commands/iphonesimulator/*; do
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
	codesign --force --sign - "$framework" 2>/dev/null
done
codesign --force --sign - "$app" 2>/dev/null
echo "built $app with $(find "$app/Frameworks" -name '*.framework' -type d | wc -l | tr -d ' ') command frameworks"
