# Clipway

[English README](./README.md)

Clipway 是一个运行在 Linux Wayland 环境下的剪切板历史应用，使用 Rust、GTK4/libadwaita、SQLite、`wl-clipboard` 和 StatusNotifier tray 实现。

当前能力：

- 持久化保存文本和图片剪切板历史到 SQLite
- 后台 daemon 模式，关闭窗口后仍可继续记录剪切板
- GTK4/libadwaita 图形界面，支持搜索、PNG 缩略图、删除、清空，以及一键复制回剪切板
- 托盘常驻模式，无需一直打开主窗口
- 提供简单 CLI，可列出、清空和恢复历史项

## 依赖

在 Arch Linux 上：

```bash
sudo pacman -S --needed gtk4 libadwaita wl-clipboard xdg-desktop-portal
```

还需要安装 Rust 和 Cargo。

关于托盘图标：

- KDE、Hyprland 以及许多支持 SNI 的桌面环境通常可以直接显示
- GNOME 一般需要安装 AppIndicator 或 StatusNotifier 扩展后才能显示托盘图标

## CI

GitHub Actions CI 配置位于 [`.github/workflows/ci.yml`](./.github/workflows/ci.yml)。
当前会自动检查：

- `cargo fmt --check`
- `cargo check --locked`
- `cargo build --locked --release`
- `cargo test --locked --no-run`
- `.desktop` 文件合法性
- shell 脚本语法
- 本地安装 smoke test

## 开发运行

```bash
cargo run
```

这会启动 GUI，并在后台 daemon 未运行时自动拉起它。

托盘模式：

```bash
cargo run -- tray
```

## CLI 命令

```bash
cargo run -- daemon
cargo run -- tray
cargo run -- list
cargo run -- list 50
cargo run -- copy 12
cargo run -- clear
```

## Release 构建

```bash
cargo build --release
./target/release/clipway
```

## 安装说明

`packaging/linux` 目录包含：

- 桌面启动器
- tray 自动启动 desktop 文件
- daemon 自动启动 desktop 文件
- systemd user service
- 本地安装与系统安装脚本

用户本地安装：

```bash
./packaging/linux/install-local.sh --with-autostart
```

系统范围安装：

```bash
sudo ./packaging/linux/install-system.sh
```

两个安装脚本都会额外安装一个辅助命令：`clipway-self-check`。

便携式 release 打包：

```bash
./packaging/linux/package-release.sh
```

它会在 `dist/` 下生成一个压缩包，例如：

```bash
dist/clipway-0.1.0-linux-x86_64.tar.gz
```

解压后可以直接运行其中的 `bin/clipway`，也可以把 `bin/clipway` 和 `bin/clipway-self-check` 拷贝到你想要的安装前缀。

## 安装后自检

安装完成后可执行：

```bash
clipway-self-check
```

如果你安装到了 `~/.local/bin`，但当前 shell 还没有把它加入 `PATH`，可以直接执行：

```bash
~/.local/bin/clipway-self-check
```

或者针对某个自定义二进制路径执行：

```bash
./packaging/linux/self-check.sh /absolute/path/to/clipway
```

自检会报告：

- `clipway` 二进制是否可调用
- `wl-copy` 和 `wl-paste` 是否已安装
- 当前是否处于 Wayland 会话
- 用户 D-Bus 上是否可见 `xdg-desktop-portal` 和 GlobalShortcuts 接口
- 是否存在 StatusNotifier watcher，以支持 tray 模式
- 针对 GNOME、KDE Plasma 和 wlroots 桌面的兼容提示

`WARN` 表示该桌面上的某些功能可能会降级。`FAIL` 表示安装本身不完整。

## 桌面兼容性

Clipway 面向 Linux Wayland。当前兼容性大致如下：

- KDE Plasma Wayland：当前最佳目标环境。剪切板历史、tray 模式以及基于 portal 的能力都更匹配这个桌面
- GNOME Wayland：剪切板捕获可以正常工作，但 tray 模式通常需要 AppIndicator 或 StatusNotifier 扩展
- wlroots 桌面，例如 Hyprland、Sway：剪切板捕获可用，但 tray 是否可见取决于面板或状态栏是否实现了 StatusNotifier
- X11 会话：不支持

一些实际使用建议：

- 如果某个桌面没有 tray 支持，`clipway daemon` 是最稳妥的退化运行方式
- 依赖 portal 的功能会受桌面实现和 portal 后端版本影响
- 如果 `clipway-self-check` 提示 tray 或 portal 不可用，剪切板历史核心功能仍然可以继续使用

## 当前限制

当前版本支持文本和 `image/png` 剪切板历史。Rich Text、文件列表以及其他 MIME 类型还没有实现。
