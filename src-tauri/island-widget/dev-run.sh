#!/bin/zsh
# Local dev loop for the island build, as `com.pais.handy.island` so it never
# collides with an installed Handy. Needs DEVELOPMENT_TEAM and SIGNING_IDENTITY
# (the same Apple Development identity for the app and its extension). Builds the widget and the app, copies the
# app to target/island-dev (the bundle path keeps stale LaunchServices records
# across identifier changes), re-registers it, resets chronod's widget cache,
# and launches it hidden. Quit an installed Handy first: they share the mic.
set -eu
cd "$(dirname "$0")/../.."
export PATH="$(brew --prefix rustup)/bin:$PATH"
LSR=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
B=src-tauri/target/debug/bundle/macos/Handy.app
D=src-tauri/target/island-dev/Handy.app
P=$(pgrep -f 'island-dev/Handy.app/Contents|bundle/macos/Handy.app/Contents' || true)
[ -n "$P" ] && kill ${=P} && sleep 1
HANDY_BUNDLE_ID=com.pais.handy.island WIDGET_CONFIGURATION=${WIDGET_CONFIGURATION:-Release} \
  DEVELOPMENT_TEAM=${DEVELOPMENT_TEAM:?set DEVELOPMENT_TEAM} src-tauri/island-widget/build.sh
CONFIG="${TMPDIR:-/tmp}/handy-island-dev.json"
cat > "$CONFIG" <<JSON
{"identifier": "com.pais.handy.island",
 "bundle": {"macOS": {
   "files": {"PlugIns/HandyIslandWidget.appex": "island-widget/build/HandyIslandWidget.appex"},
   "signingIdentity": "${SIGNING_IDENTITY:?set SIGNING_IDENTITY to a codesigning identity or its SHA-1}"}}}
JSON
CARGO_PROFILE_DEV_DEBUG_ASSERTIONS=false CMAKE_POLICY_VERSION_MINIMUM=3.5 \
  bun run tauri build --debug --bundles app --config "$CONFIG" \
  > "${TMPDIR:-/tmp}/handy-island-build.log" 2>&1 || true
# Tauri exits non-zero at its updater-signing step (no key locally), after the
# .app is bundled; anything that fails earlier never prints this line.
grep -q "Bundling Handy.app" "${TMPDIR:-/tmp}/handy-island-build.log" || {
  echo "tauri build failed before bundling; see ${TMPDIR:-/tmp}/handy-island-build.log" >&2; exit 1; }
codesign --verify --deep --strict "$B"
$LSR -u "$B" 2>/dev/null || true
rm -rf "$D" && mkdir -p "$(dirname "$D")" && cp -R "$B" "$D"
$LSR -f -R "$D"
killall chronod 2>/dev/null || true
sleep 2
open -g "$D" --args --start-hidden
echo "running $D"
