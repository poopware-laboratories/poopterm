#!/bin/sh
# Builds Poopterm for the iOS simulator as an app bundle, with the command
# libraries (ios/commands/build.sh) in Poopterm.app/commands, where the
# in-process shell finds them on $PATH:
#
#   ios/bundle-sim.sh && xcrun simctl install booted target/ios-sim/Poopterm.app
#
# The binary is marked as built against the iOS 18 SDK, since the app has
# no scene delegate yet and iOS 27 requires one of apps built for it.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
root=$(cd "$here/.." && pwd)
cd "$root"
cargo build --target aarch64-apple-ios-sim
"$here/commands/build.sh" iphonesimulator
app=target/ios-sim/Poopterm.app
rm -rf "$app"
mkdir -p "$app/commands"
cp "$here/Info.plist" "$app/Info.plist"
vtool -set-build-version iossim 17.0 18.0 -replace -output "$app/Poopterm" \
	target/aarch64-apple-ios-sim/debug/poopterm 2>/dev/null
chmod +x "$app/Poopterm"
for command in target/ios-commands/iphonesimulator/*; do
	cp "$command" "$app/commands/"
	codesign --force --sign - "$app/commands/$(basename "$command")" 2>/dev/null
done
codesign --force --sign - "$app" 2>/dev/null
echo "built $app with $(ls "$app/commands" | wc -l | tr -d ' ') commands"
