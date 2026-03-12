#!/usr/bin/env sh
set -eu

ROOT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
PREFIX=${PREFIX:-/usr/local}
BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
SYSTEMD_DIR="$PREFIX/lib/systemd/user"

usage() {
    cat <<'EOF'
Usage: install-system.sh [options]

Options:
  --prefix=PATH   Install under PATH instead of /usr/local
EOF
}

for arg in "$@"; do
    case "$arg" in
        --prefix=*)
            PREFIX=${arg#*=}
            BIN_DIR="$PREFIX/bin"
            APP_DIR="$PREFIX/share/applications"
            SYSTEMD_DIR="$PREFIX/lib/systemd/user"
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

install -d "$BIN_DIR" "$APP_DIR" "$SYSTEMD_DIR"
install -m 755 "$ROOT_DIR/target/release/clipway" "$BIN_DIR/clipway"
install -m 755 "$ROOT_DIR/packaging/linux/self-check.sh" "$BIN_DIR/clipway-self-check"

sed "s|Exec=clipway$|Exec=$BIN_DIR/clipway|g" \
    "$ROOT_DIR/packaging/linux/clipway.desktop" \
    > "$APP_DIR/clipway.desktop"

sed "s|ExecStart=/usr/bin/env clipway daemon|ExecStart=$BIN_DIR/clipway daemon|g" \
    "$ROOT_DIR/packaging/linux/clipway.service" \
    > "$SYSTEMD_DIR/clipway.service"

echo "Installed Clipway into $PREFIX"
