#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
ARCH=${ARCH:-$(uname -m)}
VERSION=${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT_DIR/Cargo.toml" | head -n 1)}
DIST_DIR=${DIST_DIR:-"$ROOT_DIR/dist"}
PACKAGE_BASENAME="clipway-${VERSION}-linux-${ARCH}"
STAGE_DIR="$DIST_DIR/$PACKAGE_BASENAME"
BIN_DIR="$STAGE_DIR/bin"
APP_DIR="$STAGE_DIR/share/applications"
ARCHIVE_PATH="$DIST_DIR/$PACKAGE_BASENAME.tar.gz"
CHECKSUM_PATH="$ARCHIVE_PATH.sha256"

cd "$ROOT_DIR"
cargo build --locked --release

rm -rf "$STAGE_DIR"
mkdir -p "$BIN_DIR" "$APP_DIR"
install -m 755 "$ROOT_DIR/target/release/clipway" "$BIN_DIR/clipway"
install -m 755 "$ROOT_DIR/packaging/linux/self-check.sh" "$BIN_DIR/clipway-self-check"
install -m 644 "$ROOT_DIR/README.md" "$STAGE_DIR/README.md"
install -m 644 "$ROOT_DIR/README.zh-CN.md" "$STAGE_DIR/README.zh-CN.md"

sed "s|Exec=clipway$|Exec=clipway|g" \
    "$ROOT_DIR/packaging/linux/clipway.desktop" \
    > "$APP_DIR/clipway.desktop"

sed "s|Exec=clipway tray|Exec=clipway tray|g" \
    "$ROOT_DIR/packaging/linux/clipway-tray.desktop" \
    > "$APP_DIR/clipway-tray.desktop"

tar -C "$DIST_DIR" -czf "$ARCHIVE_PATH" "$PACKAGE_BASENAME"
(
    cd "$DIST_DIR"
    sha256sum "$(basename "$ARCHIVE_PATH")" > "$(basename "$CHECKSUM_PATH")"
)

printf '%s\n' "$ARCHIVE_PATH"
printf '%s\n' "$CHECKSUM_PATH"
