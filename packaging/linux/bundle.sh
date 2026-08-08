#!/usr/bin/env bash
#
# Builds the Linux release tarball.
#
#   ./packaging/linux/bundle.sh
#
# Output: target/release/bundle/HeadroomLab-<version>-linux-<arch>.tar.gz
#
# The archive carries the binary, the icons, the desktop entry and install.sh.
# The updater only ever replaces the *binary* — icons and the desktop entry are
# a first-install concern, which is why they are not repackaged on every update.

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
case "$(uname -m)" in
  x86_64) ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *) echo "error: unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

TARGET="linux-${ARCH}"
STAGE="HeadroomLab-${VERSION}-${TARGET}"
OUT="target/release/bundle"

echo "==> Building HeadroomLab $VERSION for $TARGET"
HL_UPDATE_ENABLED=1 cargo build --release

echo "==> Staging $STAGE"
rm -rf "${OUT:?}/$STAGE"
mkdir -p "$OUT/$STAGE"
install -m 755 target/release/HeadroomLab "$OUT/$STAGE/HeadroomLab"
install -m 644 packaging/linux/headroomlab.desktop "$OUT/$STAGE/headroomlab.desktop"
install -m 755 packaging/linux/install.sh "$OUT/$STAGE/install.sh"
cp -R packaging/linux/hicolor "$OUT/$STAGE/hicolor"

echo "==> Packing $STAGE.tar.gz"
tar -czf "$OUT/$STAGE.tar.gz" -C "$OUT" "$STAGE"

echo
echo "Done: $OUT/$STAGE.tar.gz"
echo "Upload it to the v${VERSION} release."
