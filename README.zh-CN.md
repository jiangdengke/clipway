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

它会在 `dist/` 下生成压缩包和校验文件，例如：

```bash
dist/clipway-0.1.0-linux-x86_64.tar.gz
dist/clipway-0.1.0-linux-x86_64.tar.gz.sha256
```

解压后可以直接运行其中的 `bin/clipway`，也可以把 `bin/clipway` 和 `bin/clipway-self-check` 拷贝到你想要的安装前缀。

## GitHub Releases

CI 会在每次 push 和 pull request 时生成构建产物。如果你要把同样的产物自动发布到 GitHub Releases，推一个和 `Cargo.toml` 版本一致的 tag 就行，例如：

```bash
git tag v0.1.0
git push origin v0.1.0
```

Release workflow 会构建压缩包、生成 `.sha256` 校验文件，并把这两个文件挂到对应 tag 的 GitHub Release 上。

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
- wlroots 桌面，例如 Hyprland、Sway、niri：剪切板捕获可用，但 tray 是否可见取决于面板或状态栏是否实现了 StatusNotifier
- X11 会话：不支持

一些实际使用建议：

- 如果某个桌面没有 tray 支持，`clipway daemon` 是最稳妥的退化运行方式
- 依赖 portal 的功能会受桌面实现和 portal 后端版本影响
- 如果 `clipway-self-check` 提示 tray 或 portal 不可用，剪切板历史核心功能仍然可以继续使用

## 呼出快捷键

在 wlroots compositor 下，更实际的方式不是让应用自己监听全局按键，而是让 compositor 绑定一个快捷键到 `clipway gui`。这个命令本身已经支持“有窗口就切到前台或切换，没有窗口就启动”。

niri 的例子，写到 `~/.config/niri/config.kdl` 里的 `binds {}`：

```kdl
binds {
    Mod+V hotkey-overlay-title="Clipboard History" { spawn "~/.local/bin/clipway" "gui"; }
}
```

补充几点：

- 如果 `clipway` 已经在 `PATH` 里，也可以写成 `spawn "clipway" "gui";`
- niri 的 `spawn` 不经过 shell，所以二进制路径和每个参数都要分开写
- niri 配置支持热重载；如果你想先检查语法，可以执行 `niri validate`
- 如果你希望登录后就开始记录历史，建议同时开启 tray 自启动，或者把 daemon 放进会话启动项

## 当前限制

当前版本支持文本和 `image/png` 剪切板历史。Rich Text、文件列表以及其他 MIME 类型还没有实现。
