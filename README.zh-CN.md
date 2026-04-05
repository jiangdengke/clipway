# Clipway

[English README](./README.md)

## 介绍

Clipway 是一个面向 Linux Wayland 的剪切板历史工具，使用 Rust、GTK4/libadwaita、SQLite、`wl-clipboard`、`gtk4-layer-shell` 和 StatusNotifier tray 构建。

它的目标不是做一个常驻的大窗口应用，而是提供更接近 rofi 的呼出式面板体验：

- 后台持续记录文本和图片剪切板历史
- 需要时快速呼出面板搜索、预览、恢复历史项
- 可选 tray 常驻，或者退化为纯 daemon 模式
- 保留简单 CLI，方便脚本化和调试

## 功能亮点

- 文本和 `image/png` 剪切板历史持久化到 SQLite
- 后台 daemon 模式，关闭窗口后仍继续记录
- 基于 `gtk4-layer-shell` 的顶部弹出面板，支持搜索、缩略图、删除和清空
- 点击历史项后自动复制回剪切板并收起
- tray 常驻模式，无需一直保留主窗口
- CLI 支持列出、恢复、清空历史

## 安装

### 运行时依赖

Arch Linux 示例：

```bash
sudo pacman -S --needed gtk4 gtk4-layer-shell libadwaita wl-clipboard xdg-desktop-portal
```

补充说明：

- 如果需要从源码构建，还需要安装 Rust 和 Cargo
- KDE、Hyprland 以及许多支持 SNI 的桌面通常可以直接显示 tray
- GNOME 通常需要 AppIndicator 或 StatusNotifier 扩展才能显示 tray 图标

### 最终用户安装

如果仓库已经发布了 GitHub Release，推荐直接安装预编译版本：

```bash
curl -fsSL https://raw.githubusercontent.com/jiangdengke/clipway/main/packaging/linux/install-release.sh | sh -s -- --with-autostart
```

这会下载最新 release，安装到 `~/.local/bin`，并写入 tray 自启动项。

如果你不想用管道执行，也可以先下载脚本再运行：

```bash
curl -fsSLO https://raw.githubusercontent.com/jiangdengke/clipway/main/packaging/linux/install-release.sh
sh install-release.sh --with-autostart
```

如果当前仓库还没有 release，就暂时只能拉源码安装：

```bash
git clone https://github.com/jiangdengke/clipway.git
cd clipway
./packaging/linux/install-local.sh --with-autostart
```

### 从源码安装

用户本地安装：

```bash
./packaging/linux/install-local.sh --with-autostart
```

系统范围安装：

```bash
sudo ./packaging/linux/install-system.sh
```

这两个脚本都会：

- 构建 release 二进制
- 安装 `clipway`
- 安装 `clipway-self-check`

`install-local.sh` 额外支持：

- `--prefix=PATH`
- `--with-autostart`
- `--with-systemd`

## 使用方法

### 最常见的用法

打开面板，并在需要时自动拉起后台监听：

```bash
clipway
```

只呼出面板：

```bash
clipway gui
```

启动 tray 常驻模式：

```bash
clipway tray
```

只启动后台监听，不启用 tray：

```bash
clipway daemon
```

### 命令行

```bash
clipway list
clipway list 50
clipway copy 12
clipway clear
clipway help
```

含义分别是：

- `list`：列出最近的历史项
- `copy <id>`：把某条历史重新复制回剪切板
- `clear`：清空全部历史
- `help`：查看帮助

### 推荐使用方式

比较实用的组合一般是：

1. 登录时启动 `clipway tray`，或者运行 `clipway daemon`
2. 给 `clipway gui` 绑定一个全局快捷键
3. 用面板完成搜索、预览和回贴

在 wlroots compositor 下，更推荐由 compositor 自己绑定快捷键，而不是让应用监听全局按键。niri 示例：

```kdl
binds {
    Alt+V allow-inhibiting=false { spawn "~/.local/bin/clipway" "gui"; }
}
```

补充说明：

- 如果 `clipway` 已在 `PATH` 中，也可以写成 `spawn "clipway" "gui";`
- `clipway gui` 只负责呼出面板，不会自动显示 tray 图标
- 如果桌面不支持 tray，`clipway daemon` 是最稳妥的退化方案

## 安装后自检

安装完成后建议执行：

```bash
clipway-self-check
```

如果 `~/.local/bin` 还没进当前 shell 的 `PATH`，可以直接执行：

```bash
~/.local/bin/clipway-self-check
```

也可以手动指定二进制路径：

```bash
./packaging/linux/self-check.sh /absolute/path/to/clipway
```

自检会检查：

- `clipway` 是否可调用
- `wl-copy` 和 `wl-paste` 是否已安装
- 当前是否处于 Wayland 会话
- `xdg-desktop-portal` 和 GlobalShortcuts 接口是否可见
- tray 所需的 StatusNotifier watcher 是否存在

`WARN` 表示某些能力会降级，`FAIL` 表示安装本身不完整。

## 桌面兼容性

Clipway 当前只面向 Linux Wayland：

- KDE Plasma Wayland：目前最合适的目标桌面
- GNOME Wayland：剪切板捕获可用，但 tray 常常需要额外扩展
- wlroots 桌面，例如 Hyprland、Sway、niri：核心功能可用，tray 是否显示取决于面板或状态栏是否支持 StatusNotifier
- X11：不支持

## 当前限制

当前版本支持：

- 文本
- `image/png`

暂不支持：

- Rich Text
- 文件列表
- 其他 MIME 类型

## 开发与发布

开发运行：

```bash
cargo run
```

tray 模式：

```bash
cargo run -- tray
```

打包 release：

```bash
./packaging/linux/package-release.sh
```

这会在 `dist/` 下生成：

```bash
dist/clipway-0.1.0-linux-x86_64.tar.gz
dist/clipway-0.1.0-linux-x86_64.tar.gz.sha256
```

如果你要把它发布到 GitHub Releases，推送一个和 `Cargo.toml` 版本一致的 tag 即可：

```bash
git tag v0.1.0
git push origin v0.1.0
```

仓库中的 CI 位于 [`.github/workflows/ci.yml`](./.github/workflows/ci.yml)，release 发布流程位于 [`.github/workflows/release.yml`](./.github/workflows/release.yml)。

## 致谢

Clipway 站在这些项目和生态之上：

- Rust
- GTK4 和 libadwaita
- `gtk4-layer-shell`
- `wl-clipboard`
- SQLite 和 `rusqlite`
- StatusNotifier 与 `ksni`
- Wayland 与 `xdg-desktop-portal` 生态

## 许可证

当前仓库还没有附带正式的 `LICENSE` 文件。

这意味着外部使用者不应默认假定本项目已经以某个开源许可证发布。如果你计划公开分发、接受外部贡献或允许第三方打包，建议尽快补充明确的许可证文本。
