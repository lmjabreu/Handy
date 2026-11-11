#!/bin/sh
# Writes copies of the SDK's ActivityKit and WidgetKit Swift interfaces with
# their `@available(macOS, unavailable)` markers turned into macOS 27.0, into
# $1. The macOS 27.2 SDK marks Live Activities unavailable although macOS
# 27.2 ships them; Apple documents them as public from macOS 27.3. Prints
# nothing and writes nothing when the SDK already offers them on macOS, so
# builds against the 27.3 SDK use it as-is.
# Usage: make-overrides.sh <out-dir> [sdk-path]
set -eu
out=$1
sdk=${2:-$(xcrun --sdk macosx --show-sdk-path)}
ak=$(find "$sdk/System/Library/Frameworks/ActivityKit.framework" -name 'arm64e-apple-macos.swiftinterface' | head -1)
[ -n "$ak" ] || { echo "make-overrides: ActivityKit interface not found in $sdk" >&2; exit 1; }
if ! grep -q '@available(macOS, unavailable)' "$ak"; then
  exit 0
fi
for module in ActivityKit WidgetKit; do
  src=$(find "$sdk/System/Library/Frameworks/$module.framework" -name 'arm64e-apple-macos.swiftinterface' | head -1)
  dir="$out/$module.swiftmodule"
  mkdir -p "$dir"
  sed -E \
    -e 's/@available\(macOS, unavailable\)/@available(macOS 27.0, *)/g' \
    -e 's/@available\(macCatalyst, unavailable\)//g' \
    -e '/^\/\/ swift-module-flags:/s/ -target [^ ]+/ -target arm64-apple-macos27.2/' \
    -e '/^\/\/ swift-module-flags:/s/ -target-variant [^ ]+//' \
    -e '/^\/\/ swift-module-flags:/s/ -target-arch-variant [^ ]+//' \
    "$src" > "$dir/arm64-apple-macos.swiftinterface"
done
echo "$out"
