#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
PREFIX=${PREFIX:-"$HOME/.local"}
BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
AUTOSTART_DIR="$HOME/.config/autostart"
SYSTEMD_DIR="$HOME/.config/systemd/user"
WITH_AUTOSTART=0
WITH_SYSTEMD=0

usage() {
    cat <<'EOF'
Usage: install-local.sh [options]

Options:
  --prefix=PATH       Install under PATH instead of ~/.local
  --with-autostart    Install the tray autostart desktop entry
  --with-systemd      Install the user systemd service file
EOF
}

for arg in "$@"; do
    case "$arg" in
        --prefix=*)
            PREFIX=${arg#*=}
            BIN_DIR="$PREFIX/bin"
            APP_DIR="$PREFIX/share/applications"
            ;;
        --with-autostart)
            WITH_AUTOSTART=1
            ;;
        --with-systemd)
            WITH_SYSTEMD=1
            ;;
        --help|-h)
            usage
            exit 0
            ;;
        *)
            echo "Unknown option: $arg" >&2
            usage >&2
            exit 1
            ;;
    esac
done

cd "$ROOT_DIR"
cargo build --release

install -d "$BIN_DIR" "$APP_DIR"
install -m 755 "$ROOT_DIR/target/release/clipway" "$BIN_DIR/clipway"
install -m 755 "$ROOT_DIR/packaging/linux/self-check.sh" "$BIN_DIR/clipway-self-check"

sed "s|Exec=clipway$|Exec=$BIN_DIR/clipway|g" \
    "$ROOT_DIR/packaging/linux/clipway.desktop" \
    > "$APP_DIR/clipway.desktop"

if [ "$WITH_AUTOSTART" -eq 1 ]; then
    install -d "$AUTOSTART_DIR"
    sed "s|Exec=clipway tray|Exec=$BIN_DIR/clipway tray|g" \
        "$ROOT_DIR/packaging/linux/clipway-tray.desktop" \
        > "$AUTOSTART_DIR/clipway-tray.desktop"
fi

if [ "$WITH_SYSTEMD" -eq 1 ]; then
    install -d "$SYSTEMD_DIR"
    sed "s|ExecStart=/usr/bin/env clipway daemon|ExecStart=$BIN_DIR/clipway daemon|g" \
        "$ROOT_DIR/packaging/linux/clipway.service" \
        > "$SYSTEMD_DIR/clipway.service"
fi

echo "Installed Clipway into $PREFIX"
echo "Run: $BIN_DIR/clipway-self-check"

if ! printf '%s' ":$PATH:" | grep -q ":$BIN_DIR:"; then
    echo "Note: $BIN_DIR is not in PATH for this shell."
    echo "Add it to your shell profile or run Clipway with absolute paths."
fi
