#!/usr/bin/env sh
set -eu

REPO=${REPO:-jiangdengke/clipway}
PREFIX=${PREFIX:-"$HOME/.local"}
BIN_DIR="$PREFIX/bin"
APP_DIR="$PREFIX/share/applications"
AUTOSTART_DIR="$HOME/.config/autostart"
SYSTEMD_DIR="$HOME/.config/systemd/user"
VERSION=
WITH_AUTOSTART=0
WITH_SYSTEMD=0

usage() {
    cat <<'EOF'
Usage: install-release.sh [options]

Options:
  --version=VERSION    Install a specific release version, for example 0.1.0 or v0.1.0
  --prefix=PATH        Install under PATH instead of ~/.local
  --repo=OWNER/REPO    Download releases from a different GitHub repository
  --with-autostart     Install the tray autostart desktop entry
  --with-systemd       Install the user systemd service file
EOF
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        echo "Missing required command: $1" >&2
        exit 1
    fi
}

detect_arch() {
    case "$(uname -m)" in
        x86_64|amd64)
            printf '%s\n' "x86_64"
            ;;
        aarch64|arm64)
            printf '%s\n' "aarch64"
            ;;
        *)
            uname -m
            ;;
    esac
}

resolve_tag() {
    if [ -n "$VERSION" ]; then
        case "$VERSION" in
            v*)
                printf '%s\n' "$VERSION"
                ;;
            *)
                printf 'v%s\n' "$VERSION"
                ;;
        esac
        return 0
    fi

    response=$(curl -fsSL \
        -H "Accept: application/vnd.github+json" \
        "https://api.github.com/repos/$REPO/releases/latest") || {
        echo "Could not query the latest GitHub release for $REPO." >&2
        echo "If no release exists yet, create one first or pass --version." >&2
        exit 1
    }

    tag=$(printf '%s\n' "$response" \
        | sed -n 's/^[[:space:]]*"tag_name":[[:space:]]*"\([^"]*\)",/\1/p' \
        | head -n 1)

    if [ -z "$tag" ]; then
        echo "No GitHub release found for $REPO." >&2
        echo "Fallback: install from source with ./packaging/linux/install-local.sh" >&2
        exit 1
    fi

    printf '%s\n' "$tag"
}

for arg in "$@"; do
    case "$arg" in
        --version=*)
            VERSION=${arg#*=}
            ;;
        --prefix=*)
            PREFIX=${arg#*=}
            BIN_DIR="$PREFIX/bin"
            APP_DIR="$PREFIX/share/applications"
            ;;
        --repo=*)
            REPO=${arg#*=}
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

require_command curl
require_command install
require_command mktemp
require_command sed
require_command tar

ARCH=$(detect_arch)
TAG=$(resolve_tag)
VERSION_STRIPPED=${TAG#v}
PACKAGE_BASENAME="clipway-${VERSION_STRIPPED}-linux-${ARCH}"
ARCHIVE_URL="https://github.com/$REPO/releases/download/$TAG/$PACKAGE_BASENAME.tar.gz"
TMP_DIR=$(mktemp -d)
ARCHIVE_PATH="$TMP_DIR/$PACKAGE_BASENAME.tar.gz"
STAGE_DIR="$TMP_DIR/$PACKAGE_BASENAME"

cleanup() {
    rm -rf "$TMP_DIR"
}

trap cleanup EXIT INT TERM

echo "Downloading $ARCHIVE_URL"
curl -fL "$ARCHIVE_URL" -o "$ARCHIVE_PATH" || {
    echo "Failed to download release archive." >&2
    echo "Make sure $TAG exists and includes an asset for $ARCH." >&2
    exit 1
}

tar -C "$TMP_DIR" -xzf "$ARCHIVE_PATH"

if [ ! -x "$STAGE_DIR/bin/clipway" ]; then
    echo "Release archive does not contain bin/clipway." >&2
    exit 1
fi

install -d "$BIN_DIR" "$APP_DIR"
install -m 755 "$STAGE_DIR/bin/clipway" "$BIN_DIR/clipway"
install -m 755 "$STAGE_DIR/bin/clipway-self-check" "$BIN_DIR/clipway-self-check"

sed "s|Exec=clipway$|Exec=$BIN_DIR/clipway|g" \
    "$STAGE_DIR/share/applications/clipway.desktop" \
    > "$APP_DIR/clipway.desktop"

if [ "$WITH_AUTOSTART" -eq 1 ]; then
    install -d "$AUTOSTART_DIR"
    sed "s|Exec=clipway tray|Exec=$BIN_DIR/clipway tray|g" \
        "$STAGE_DIR/share/applications/clipway-tray.desktop" \
        > "$AUTOSTART_DIR/clipway-tray.desktop"
fi

if [ "$WITH_SYSTEMD" -eq 1 ]; then
    if [ ! -f "$STAGE_DIR/lib/systemd/user/clipway.service" ]; then
        echo "Release archive does not contain a systemd user service template." >&2
        exit 1
    fi

    install -d "$SYSTEMD_DIR"
    sed "s|ExecStart=/usr/bin/env clipway daemon|ExecStart=$BIN_DIR/clipway daemon|g" \
        "$STAGE_DIR/lib/systemd/user/clipway.service" \
        > "$SYSTEMD_DIR/clipway.service"
fi

echo "Installed Clipway $VERSION_STRIPPED into $PREFIX"
echo "Run: $BIN_DIR/clipway-self-check"

if [ "$WITH_AUTOSTART" -eq 1 ]; then
    echo "Tray autostart installed into $AUTOSTART_DIR"
fi

if [ "$WITH_SYSTEMD" -eq 1 ]; then
    echo "To enable the user service:"
    echo "  systemctl --user daemon-reload"
    echo "  systemctl --user enable --now clipway.service"
fi

if ! printf '%s' ":$PATH:" | grep -q ":$BIN_DIR:"; then
    echo "Note: $BIN_DIR is not in PATH for this shell."
    echo "Add it to your shell profile or run Clipway with absolute paths."
fi
