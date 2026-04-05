# Clipway

[中文说明](./README.zh-CN.md)

## Introduction

Clipway is a clipboard history tool for Linux on Wayland, built with Rust, GTK4/libadwaita, SQLite, `wl-clipboard`, `gtk4-layer-shell`, and a StatusNotifier tray entry.

The project is aimed at a popup-first workflow rather than a traditional always-open window:

- keep collecting clipboard history in the background
- summon a compact panel when you need to search or restore items
- optionally stay resident in the tray, or fall back to daemon-only mode
- keep a small CLI for scripting and debugging

## Highlights

- Persistent clipboard history for text and `image/png`
- Background daemon mode that continues collecting after the GUI closes
- Top popup panel built with `gtk4-layer-shell`, with search, thumbnails, delete, and clear actions
- Copy-back interaction that restores an item and hides the panel
- Tray resident mode for quick access without leaving a main window open
- Small CLI for listing, restoring, and clearing history

## Installation

### Runtime Dependencies

Arch Linux example:

```bash
sudo pacman -S --needed gtk4 gtk4-layer-shell libadwaita wl-clipboard xdg-desktop-portal
```

Notes:

- Rust and Cargo are required if you want to build from source
- KDE, Hyprland, and many SNI-capable desktops can usually show the tray directly
- GNOME usually needs an AppIndicator or StatusNotifier extension for the tray icon

### End-User Install

If the repository already has GitHub Releases, the preferred path is to install the prebuilt package:

```bash
curl -fsSL https://raw.githubusercontent.com/jiangdengke/clipway/main/packaging/linux/install-release.sh | sh -s -- --with-autostart
```

This downloads the latest release, installs it into `~/.local/bin`, and writes the tray autostart entry.

If you do not want to pipe into `sh`, download the script first:

```bash
curl -fsSLO https://raw.githubusercontent.com/jiangdengke/clipway/main/packaging/linux/install-release.sh
sh install-release.sh --with-autostart
```

If the repository does not have a release yet, the fallback is to clone and install from source:

```bash
git clone https://github.com/jiangdengke/clipway.git
cd clipway
./packaging/linux/install-local.sh --with-autostart
```

### Install From Source

User-local install:

```bash
./packaging/linux/install-local.sh --with-autostart
```

System-wide install:

```bash
sudo ./packaging/linux/install-system.sh
```

Both scripts:

- build the release binary
- install `clipway`
- install `clipway-self-check`

`install-local.sh` also supports:

- `--prefix=PATH`
- `--with-autostart`
- `--with-systemd`

## Usage

### Common Commands

Open the popup panel and ensure the background watcher is running:

```bash
clipway
```

Open only the panel:

```bash
clipway gui
```

Start tray resident mode:

```bash
clipway tray
```

Run only the background watcher, without the tray:

```bash
clipway daemon
```

### CLI

```bash
clipway list
clipway list 50
clipway copy 12
clipway clear
clipway help
```

What they do:

- `list`: print recent items
- `copy <id>`: restore one saved item back to the clipboard
- `clear`: remove the full history
- `help`: print usage

### Recommended Workflow

A practical setup is:

1. start `clipway tray` at login, or run `clipway daemon`
2. bind a global shortcut to `clipway gui`
3. use the popup panel for search, preview, and restore

On wlroots compositors, it is usually better to let the compositor own the shortcut instead of making the app listen for global keys. Example for niri:

```kdl
binds {
    Alt+V allow-inhibiting=false { spawn "~/.local/bin/clipway" "gui"; }
}
```

Notes:

- If `clipway` is already in your `PATH`, `spawn "clipway" "gui";` is enough
- `clipway gui` only opens the panel; it does not create a tray icon by itself
- If your desktop does not support tray icons well, `clipway daemon` is the safest fallback

## Post-Install Self-Check

After installation, it is worth running:

```bash
clipway-self-check
```

If `~/.local/bin` is not yet in your shell `PATH`, run:

```bash
~/.local/bin/clipway-self-check
```

Or point it to a specific binary:

```bash
./packaging/linux/self-check.sh /absolute/path/to/clipway
```

The self-check verifies:

- whether `clipway` is callable
- whether `wl-copy` and `wl-paste` are installed
- whether the current session is Wayland
- whether `xdg-desktop-portal` and GlobalShortcuts are visible
- whether a StatusNotifier watcher is available for tray support

`WARN` means a feature may degrade on the current desktop. `FAIL` means the installation is incomplete.

## Desktop Compatibility

Clipway currently targets Linux on Wayland only:

- KDE Plasma Wayland: the best-supported environment right now
- GNOME Wayland: clipboard capture works, but tray mode often needs an extra shell extension
- wlroots desktops such as Hyprland, Sway, and niri: core clipboard history works; tray visibility depends on the panel or bar implementing StatusNotifier
- X11: unsupported

## Current Limitations

Current support:

- text
- `image/png`

Not implemented yet:

- rich text
- file lists
- other MIME types

## Development And Release

Run in development:

```bash
cargo run
```

Tray mode:

```bash
cargo run -- tray
```

Build the release package:

```bash
./packaging/linux/package-release.sh
```

This produces:

```bash
dist/clipway-0.1.0-linux-x86_64.tar.gz
dist/clipway-0.1.0-linux-x86_64.tar.gz.sha256
```

To publish the package on GitHub Releases, push a tag that matches the version in `Cargo.toml`:

```bash
git tag v0.1.0
git push origin v0.1.0
```

The CI workflow lives in [`.github/workflows/ci.yml`](./.github/workflows/ci.yml) and the release workflow lives in [`.github/workflows/release.yml`](./.github/workflows/release.yml).

## Acknowledgements

Clipway builds on top of these projects and ecosystems:

- Rust
- GTK4 and libadwaita
- `gtk4-layer-shell`
- `wl-clipboard`
- SQLite and `rusqlite`
- StatusNotifier and `ksni`
- the broader Wayland and `xdg-desktop-portal` ecosystem

## License

This repository does not currently include a formal `LICENSE` file.

Until one is added, downstream users should not assume the project has been published under an open-source license. If you plan to distribute it publicly, accept external contributions, or let third parties package it, add an explicit license text first.
