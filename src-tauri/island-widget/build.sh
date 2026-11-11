#!/bin/sh
# Builds HandyIslandWidget.appex into island-widget/build/ for bundling.
# DEVELOPMENT_TEAM must match the identity Handy.app is signed with, and
# HANDY_BUNDLE_ID the app's identifier (the extension's must extend it).
set -eu
cd "$(dirname "$0")"
overrides=$(./make-overrides.sh "$PWD/build/overrides")
flags=""
[ -n "$overrides" ] && flags="-I $overrides"
xcodegen generate --quiet
xcodebuild -project HandyIslandWidget.xcodeproj -scheme HandyIslandWidget \
  -configuration "${WIDGET_CONFIGURATION:-Release}" -derivedDataPath build/dd -allowProvisioningUpdates \
  ${DEVELOPMENT_TEAM:+DEVELOPMENT_TEAM=$DEVELOPMENT_TEAM} \
  PRODUCT_BUNDLE_IDENTIFIER="${HANDY_BUNDLE_ID:-com.pais.handy}.HandyIslandWidget" \
  ISLAND_OVERRIDES_FLAGS="$flags" build -quiet
rm -rf build/HandyIslandWidget.appex
cp -R build/dd/Build/Products/${WIDGET_CONFIGURATION:-Release}/HandyIslandWidget.appex build/
echo "built $PWD/build/HandyIslandWidget.appex"
