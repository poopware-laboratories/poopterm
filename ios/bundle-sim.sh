#!/bin/sh
# Builds Poopterm for the iOS simulator as an app bundle, with the command
# libraries (ios/commands/build.sh) in Poopterm.app/commands, where the
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
mkdir -p "$app/commands"
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
	cp "$command" "$app/commands/"
	codesign --force --sign - "$app/commands/$(basename "$command")" 2>/dev/null
done
codesign --force --sign - "$app" 2>/dev/null
echo "built $app with $(ls "$app/commands" | wc -l | tr -d ' ') commands"
