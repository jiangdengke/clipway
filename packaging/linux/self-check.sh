#!/usr/bin/env sh
set -eu

PASS_COUNT=0
WARN_COUNT=0
FAIL_COUNT=0

TARGET=${1:-${CLIPWAY_BIN:-clipway}}
SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)

pass() {
    PASS_COUNT=$((PASS_COUNT + 1))
    printf 'PASS: %s\n' "$1"
}

warn() {
    WARN_COUNT=$((WARN_COUNT + 1))
    printf 'WARN: %s\n' "$1"
}

fail() {
    FAIL_COUNT=$((FAIL_COUNT + 1))
    printf 'FAIL: %s\n' "$1"
}

resolve_binary() {
    if [ -x "$TARGET" ]; then
        printf '%s\n' "$TARGET"
        return 0
    fi

    if [ "$TARGET" = "clipway" ] && [ -x "$SCRIPT_DIR/clipway" ]; then
        printf '%s\n' "$SCRIPT_DIR/clipway"
        return 0
    fi

    if command -v "$TARGET" >/dev/null 2>&1; then
        command -v "$TARGET"
        return 0
    fi

    return 1
}

if BIN_PATH=$(resolve_binary); then
    pass "Clipway binary found at $BIN_PATH"
else
    fail "Clipway binary not found. Pass an absolute path or add clipway to PATH."
    printf '\nSummary: %s passed, %s warnings, %s failed\n' "$PASS_COUNT" "$WARN_COUNT" "$FAIL_COUNT"
    exit 1
fi

if "$BIN_PATH" help >/dev/null 2>&1; then
    pass "Clipway CLI responds"
else
    fail "Clipway CLI did not respond successfully"
fi

if command -v wl-copy >/dev/null 2>&1 && command -v wl-paste >/dev/null 2>&1; then
    pass "wl-clipboard tools are installed"
else
    fail "wl-copy and wl-paste are required for clipboard integration"
fi

if [ -n "${WAYLAND_DISPLAY:-}" ]; then
    pass "Wayland session detected: $WAYLAND_DISPLAY"
else
    warn "WAYLAND_DISPLAY is not set. Clipway runs only on Wayland."
fi

if command -v busctl >/dev/null 2>&1; then
    if busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop org.freedesktop.portal.GlobalShortcuts >/dev/null 2>&1; then
        pass "xdg-desktop-portal GlobalShortcuts interface is available"
    elif busctl --user introspect org.freedesktop.portal.Desktop /org/freedesktop/portal/desktop org.freedesktop.portal.Settings >/dev/null 2>&1; then
        warn "xdg-desktop-portal is present, but GlobalShortcuts is not exposed by this desktop"
    else
        warn "Could not reach xdg-desktop-portal on the user bus"
    fi

    if busctl --user introspect org.kde.StatusNotifierWatcher /StatusNotifierWatcher org.kde.StatusNotifierWatcher >/dev/null 2>&1; then
        pass "StatusNotifier watcher is available for the tray icon"
    else
        warn "No StatusNotifier watcher detected. Tray mode may stay hidden on this desktop."
    fi
else
    warn "busctl is not available, skipping D-Bus desktop capability checks"
fi

if [ -n "${XDG_CURRENT_DESKTOP:-}" ]; then
    case "$XDG_CURRENT_DESKTOP" in
        *GNOME*)
            warn "GNOME usually needs an AppIndicator or StatusNotifier extension for tray icons"
            ;;
        *KDE*|*Plasma*)
            pass "KDE Plasma desktop detected"
            ;;
        *Hyprland*|*sway*|*Sway*)
            warn "wlroots desktops need a panel with StatusNotifier support for tray icons"
            ;;
        *)
            warn "Desktop '$XDG_CURRENT_DESKTOP' is unverified. Use daemon mode if tray support is missing."
            ;;
    esac
else
    warn "XDG_CURRENT_DESKTOP is not set, skipping desktop-specific guidance"
fi

printf '\nSummary: %s passed, %s warnings, %s failed\n' "$PASS_COUNT" "$WARN_COUNT" "$FAIL_COUNT"

if [ "$FAIL_COUNT" -gt 0 ]; then
    exit 1
fi
