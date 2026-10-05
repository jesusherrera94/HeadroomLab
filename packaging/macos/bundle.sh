#!/usr/bin/env bash
#
# Builds HeadroomLab.app and the release tarball the updater expects.
#
#   ./packaging/macos/bundle.sh
#
# Output:
#   target/release/bundle/HeadroomLab.app
#   target/release/bundle/HeadroomLab-<version>-macos-<arch>.tar.gz
#
# The archive name must match `HL_UPDATE_ASSET_PATTERN` (default
# `HeadroomLab-{version}-{target}`) or the updater will not recognise its own
# release. See RELEASING.md.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
if [[ -z "$VERSION" ]]; then
  echo "error: could not read version from Cargo.toml" >&2
  exit 1
fi

case "$(uname -m)" in
  arm64) ARCH="aarch64" ;;
  x86_64) ARCH="x86_64" ;;
  *) echo "error: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

TARGET="macos-${ARCH}"
OUT="target/release/bundle"
APP="$OUT/HeadroomLab.app"

echo "==> Building HeadroomLab $VERSION for $TARGET"
# Updates on by default in release, but stated rather than assumed: this is the
# artifact users actually run, and it is the one build that must self-update.
HL_UPDATE_ENABLED=1 cargo build --release

echo "==> Assembling $APP"
rm -rf "$OUT"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"

cp target/release/HeadroomLab "$APP/Contents/MacOS/HeadroomLab"
chmod +x "$APP/Contents/MacOS/HeadroomLab"

sed "s/__VERSION__/$VERSION/g" \
  packaging/macos/Info.plist.template > "$APP/Contents/Info.plist"

# The icon is optional: without it macOS shows the generic app icon, which is
# ugly but not broken, and blocking a release on a missing .icns would be worse.
if [[ -f packaging/macos/AppIcon.icns ]]; then
  cp packaging/macos/AppIcon.icns "$APP/Contents/Resources/AppIcon.icns"
else
  echo "    note: packaging/macos/AppIcon.icns not found — using the generic icon"
fi

# An ad-hoc signature is not notarisation and does not satisfy Gatekeeper for a
# downloaded app (see RELEASING.md). It is applied anyway because an unsigned
# bundle whose contents change can be refused outright on Apple Silicon.
if command -v codesign >/dev/null 2>&1; then
  echo "==> Ad-hoc signing"
  codesign --force --deep --sign - "$APP"
fi

ARCHIVE="HeadroomLab-${VERSION}-${TARGET}.tar.gz"
echo "==> Packing $ARCHIVE"
# -C so the archive contains HeadroomLab.app at its root, which is where the
# updater's `find_bundle` looks first.
tar -czf "$OUT/$ARCHIVE" -C "$OUT" HeadroomLab.app

echo
echo "Done:"
echo "  $OUT/HeadroomLab.app"
echo "  $OUT/$ARCHIVE"
echo
echo "Upload $ARCHIVE to the v${VERSION} release. Verify with:"
echo "  tar -tzf $OUT/$ARCHIVE | head"
