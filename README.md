# Clipway

Clipway is a Wayland clipboard history app for Linux built with Rust, GTK4/libadwaita, SQLite, `wl-clipboard`, and a StatusNotifier tray entry.

Current scope:

- Persistent text and image clipboard history stored in SQLite
- Background daemon mode so capture continues after the GUI closes
- GTK4/libadwaita viewer with search, thumbnails for PNG images, delete, clear, and one-click copy-back
- Tray resident mode so Clipway can stay available without an open window
- Small CLI for listing, clearing, and restoring saved items

## Dependencies

On Arch Linux:

```bash
sudo pacman -S --needed gtk4 libadwaita wl-clipboard xdg-desktop-portal
```

Rust and Cargo are required to build the app.

For the tray icon:

- KDE, Hyprland, and many SNI-capable desktops work directly.
- GNOME usually needs an AppIndicator/StatusNotifier extension to show the tray icon.

## CI

GitHub Actions CI is defined in [`.github/workflows/ci.yml`](./.github/workflows/ci.yml).
It currently checks:

- `cargo fmt --check`
- `cargo check --locked`
- `cargo build --locked --release`
- `cargo test --locked --no-run`
- desktop entry validation
- shell script syntax
- a local install smoke test

## Run In Development

```bash
cargo run
```

This opens the GUI and auto-starts the background daemon if it is not already running.

Tray mode:

```bash
cargo run -- tray
```

## CLI Commands

```bash
cargo run -- daemon
cargo run -- tray
cargo run -- list
cargo run -- list 50
cargo run -- copy 12
cargo run -- clear
```

## Release Build

```bash
cargo build --release
./target/release/clipway
```

## Install Notes

The `packaging/linux` directory contains:

- a desktop launcher
- a tray autostart desktop entry
- a daemon-only autostart desktop entry
- a systemd user service
- local and system install scripts

User-local install:

```bash
./packaging/linux/install-local.sh --with-autostart
```

System-wide install:

```bash
sudo ./packaging/linux/install-system.sh
```

Both install scripts also install a helper command named `clipway-self-check`.

## Post-Install Self-Check

After installation, run:

```bash
clipway-self-check
```

Or against a custom binary path:

```bash
./packaging/linux/self-check.sh /absolute/path/to/clipway
```

The self-check reports:

- whether the `clipway` binary is callable
- whether `wl-copy` and `wl-paste` are available
- whether you are currently in a Wayland session
- whether `xdg-desktop-portal` and the GlobalShortcuts interface are visible on the user bus
- whether a StatusNotifier watcher is present for tray support
- desktop-specific warnings for GNOME, KDE Plasma, and wlroots desktops

Warnings mean "feature may degrade on this desktop". Failures mean the install is incomplete.

## Desktop Compatibility

Clipway is designed for Linux on Wayland. Current support level:

- KDE Plasma Wayland: best-supported target. Clipboard history, tray mode, and portal-based integrations fit this desktop well.
- GNOME Wayland: clipboard capture works, but tray mode usually needs an AppIndicator or StatusNotifier shell extension.
- wlroots desktops such as Hyprland and Sway: clipboard capture works if `wl-clipboard` is installed; tray visibility depends on the panel or bar implementing StatusNotifier.
- X11 sessions: not supported.

Operational notes:

- `clipway daemon` is the safest fallback if tray support is missing on a desktop.
- Portal-dependent features can vary across desktop implementations and portal backend versions.
- If `clipway-self-check` warns about missing tray or portal support, the clipboard history core can still work.

## Current Limitation

This version supports text and `image/png` clipboard history. Rich text, files, and other MIME types are not implemented yet.
