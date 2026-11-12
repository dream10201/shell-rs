<p align="center">
  <img src="assets/logo/shellrs.svg" width="96" alt="ShellRS">
</p>

<h1 align="center">ShellRS</h1>

<p align="center">
  用 Rust 编写、由 GPU 渲染界面的高性能原生跨平台 SSH 客户端：Xshell 式的主机管理、多标签终端、文件上传下载和端口转发，都在一个窗口里。
</p>

<p align="center">
  <a href="https://github.com/dream10201/shell-rs/releases"><img src="https://img.shields.io/github/v/release/dream10201/shell-rs?include_prereleases&label=release" alt="Release"></a>
  <a href="#许可证"><img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License: GPL-3.0"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="Platform: macOS | Windows | Linux">
  <img src="https://img.shields.io/badge/rust-1.99-orange" alt="Rust 1.99">
  <a href="https://linux.do"><img src="https://img.shields.io/badge/LINUX-DO-FFB003.svg?logo=data:image/svg%2bxml;base64,DQo8c3ZnIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgd2lkdGg9IjEwMCIgaGVpZ2h0PSIxMDAiPjxwYXRoIGQ9Ik00Ni44Mi0uMDU1aDYuMjVxMjMuOTY5IDIuMDYyIDM4IDIxLjQyNmM1LjI1OCA3LjY3NiA4LjIxNSAxNi4xNTYgOC44NzUgMjUuNDV2Ni4yNXEtMi4wNjQgMjMuOTY4LTIxLjQzIDM4LTExLjUxMiA3Ljg4NS0yNS40NDUgOC44NzRoLTYuMjVxLTIzLjk3LTIuMDY0LTM4LjAwNC0yMS40M1EuOTcxIDY3LjA1Ni0uMDU0IDUzLjE4di02LjQ3M0MxLjM2MiAzMC43ODEgOC41MDMgMTguMTQ4IDIxLjM3IDguODE3IDI5LjA0NyAzLjU2MiAzNy41MjcuNjA0IDQ2LjgyMS0uMDU2IiBzdHlsZT0ic3Ryb2tlOm5vbmU7ZmlsbC1ydWxlOmV2ZW5vZGQ7ZmlsbDojZWNlY2VjO2ZpbGwtb3BhY2l0eToxIi8+PHBhdGggZD0iTTQ3LjI2NiAyLjk1N3EyMi41My0uNjUgMzcuNzc3IDE1LjczOGE0OS43IDQ5LjcgMCAwIDEgNi44NjcgMTAuMTU3cS00MS45NjQuMjIyLTgzLjkzIDAgOS43NS0xOC42MTYgMzAuMDI0LTI0LjM4N2E2MSA2MSAwIDAgMSA5LjI2Mi0xLjUwOCIgc3R5bGU9InN0cm9rZTpub25lO2ZpbGwtcnVsZTpldmVub2RkO2ZpbGw6IzE5MTkxOTtmaWxsLW9wYWNpdHk6MSIvPjxwYXRoIGQ9Ik03Ljk4IDcwLjkyNmMyNy45NzctLjAzNSA1NS45NTQgMCA4My45My4xMTNRODMuNDI2IDg3LjQ3MyA2Ni4xMyA5NC4wODZxLTE4LjgxIDYuNTQ0LTM2LjgzMi0xLjg5OC0xNC4yMDMtNy4wOS0yMS4zMTctMjEuMjYyIiBzdHlsZT0ic3Ryb2tlOm5vbmU7ZmlsbC1ydWxlOmV2ZW5vZGQ7ZmlsbDojZjlhZjAwO2ZpbGwtb3BhY2l0eToxIi8+PC9zdmc+" alt="LINUX DO"></a>
</p>

<p align="center"><a href="README.md">English</a> | <b>简体中文</b></p>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/main-dark.png">
  <img src="docs/screenshots/main-light.png" alt="ShellRS 主界面：左侧是分组的主机树，右侧远程终端里运行着 htop，标签栏显示连接延迟">
</picture>

## 简介

ShellRS 把 Xshell 式的主机管理和多标签终端、经终端连接的文件上传下载、SSH 端口转发放进同一个跨平台应用，JumpServer 等堡垒机也可以像调用 Xshell 一样调用它。它用 Rust 编写，界面基于 Zed 编辑器的 GPU 加速 UI 框架 GPUI（通过 [gpui-kit](https://gpui-kit.com)），不依赖 Electron 或 JVM。

主机、分组、凭据和转发规则保存在本地的 SQLite 数据库里。ShellRS 存在这台电脑上的东西（数据库、设置、私钥、保存的密码和口令）都用首次启动时设的应用锁密码加密，不使用系统钥匙串，数据库里没有任何秘密。

## 功能

**主机管理**

- 分组可以任意层级嵌套，支持拖动整理和搜索。
- 连上主机后自动识别远程系统，在主机树和标签上显示对应的系统徽章（各 Linux 发行版、BSD、macOS、Windows）。
- 认证方式三选一：密码、无密码（依次尝试服务器免认证、SSH Agent 和 `~/.ssh` 下的默认私钥）、使用凭据。
- 连接方式三选一：直接连接、多级 SSH 跳板、HTTP / SOCKS5 代理。终端、文件传输和端口转发都按它连接。
- 「测试连接」会真实登录一次，失败时写明原因。
- 除了 SSH，还有 Telnet 主机（交换机、串口服务器、实验环境的设备），按设备的提示自动登录，同样可以走跳板和代理；以及本机的串口，接 Console 线和调试口。

**终端**

- 多标签的远程终端和本地终端（基于 `alacritty_terminal`），vim、htop 等全屏程序正常显示。
- 标签栏实时显示这条 SSH 连接的往返延迟。
- 支持查找（智能大小写）和本地清屏，字体、字号、行高都可以设置。
- ZMODEM：在终端里用 `rz`、`sz` 上传下载文件，SSH、Telnet、串口都能用；把文件拖到终端上就能上传。
- 标签可以拖成左右或上下分栏，几个终端并排看。

**右侧栏工具**

- 跟着当前的远程终端，在这条终端自己的连接上读取和执行，不另外登录；⌘⌥B（其他平台 Ctrl+Alt+B）显示或隐藏。
- 系统监控：系统信息、CPU（含每个核）、内存、网卡速率和磁盘占用，每 2 秒刷新。
- 网络连接、进程管理和系统服务：搜索和筛选，查看进程详情、结束进程，启动、停止、重启 systemd 服务，开关开机启动，查看日志；不是 root 时用免密码的 sudo。这三项和系统监控只支持 Linux。
- 历史命令读主机上的 `~/.bash_history`，命令片段所有主机共用、可以分类；点一下输入到终端，或直接执行。
- tmux：主机上的 tmux 会话，标出终端所在的那个；新建、连接、退出当前、重命名和结束会话。

**文件传输**

- SSH 终端连上后，标签栏右侧的「上传」「下载文件…」经这个终端自己的连接传文件和文件夹：不再登录，跳板和代理照样走。
- 不用 SFTP，也不用 `scp`：只靠主机 shell 里的 `cat`、`wc`、`mkdir`、`mv`、`find`，服务器上没有 SFTP 子系统也能用。
- 上传先写临时文件，收全后才替换，中途断开不会留下半个文件；进度显示在状态栏，可以取消。

**从堡垒机打开**

- 兼容 Xshell 的命令行调用方式：在 JumpServer 等堡垒机的客户端里把 ShellRS 设成 SSH 客户端，`ssh://` 链接打开终端。
- 认得 Xshell 的 `-url`、`-newtab`；ShellRS 已在运行时，链接交给正在运行的那个打开。
- 打开的是不保存的外部连接：不进主机列表，链接里的密码只留在内存里，标签关掉就没了。每条连接只用一个通道，不允许多开通道的堡垒机也能用。详见[使用手册](docs/manual.zh.md#从堡垒机打开外部连接)。

**端口转发**

- 本地（`-L`）、远程（`-R`）和动态（`-D`，SOCKS5 / SOCKS4）转发。
- 配置对话框里有实时示意图和一句话说明，看得出连接从哪进、到哪去。
- 每条规则自己建立 SSH 连接，断线后自动重连三次；可以设为随 ShellRS 启动自动开启。

**凭据**

- 密码、密钥、SSH Agent 三种凭据，多台主机可以共用一条；改凭据，用它的主机一起变。
- 密钥可以引用本机文件、直接粘贴，或当场生成 Ed25519 / RSA 4096，并复制公钥。

**安全与更新**

- 全盘加密：应用锁密码加密全部数据（Argon2id、XChaCha20-Poly1305），每次启动和锁定后都要输入；不能关闭，忘了密码无法找回。
- 各系统都能当便携版用：程序（`shellrs.exe`、`.AppImage` 或 `ShellRS.app`）旁边有 `data` 文件夹时，全部数据都在里面，拿到别的电脑用同一个密码就能打开，删掉就什么都不留。
- 主机信任记录存在 ShellRS 自己的 `known_hosts` 里，不改动 `~/.ssh`。
- 在线升级：更新清单经 minisign 签名，安装包按 SHA-256 校验；后台下载，重启或退出时安装。分稳定版和 Beta 两个渠道，检查更新只发送版本号、系统和 CPU 架构。

## 截图

<p align="center">
  <img src="docs/screenshots/tools.png" alt="右侧栏的三个工具并排：系统监控、进程管理和系统服务"><br>
  <em>右侧栏工具：系统监控、进程管理和系统服务</em>
</p>

<p align="center">
  <img src="docs/screenshots/forward.png" alt="编辑端口转发对话框：本地转发的示意图和说明，左侧转发列表里两条规则正在运行"><br>
  <em>端口转发：示意图实时说明连接怎么走</em>
</p>

## 下载

到 GitHub [Releases](https://github.com/dream10201/shell-rs/releases) 下载。Beta 版在 Releases 里标为 Pre-release，想提前用上新功能，可以在「设置 › 关于」里把更新渠道切到 Beta。

| 系统 | 安装包 | 说明 |
| --- | --- | --- |
| macOS 11+（Apple Silicon） | `ShellRS-<版本>-macos-aarch64.dmg` | 已签名并经过 Apple 公证。打开 DMG，把 ShellRS 拖进「应用程序」 |
| macOS 11+（Intel） | `ShellRS-<版本>-macos-x86_64.dmg` | 已签名并经过 Apple 公证。打开 DMG，把 ShellRS 拖进「应用程序」 |
| Windows x64 | `ShellRS-<版本>-windows-x86_64-setup.exe` | 按当前用户安装，不需要管理员权限。安装程序暂未签名，SmartScreen 拦下时点「更多信息 › 仍要运行」 |
| Windows x64（便携版） | `ShellRS-<版本>-windows-x86_64-portable.zip` | 解压到自己能写入的文件夹，运行 `shellrs.exe`，数据都在旁边的 `data` 文件夹里。见[便携版](docs/manual.zh.md#便携版) |
| Linux x64 | `ShellRS-<版本>-linux-x86_64.AppImage` | `chmod +x` 后直接运行，需要 glibc 2.31 以上（Ubuntu 20.04、Debian 11 及更新的版本） |

每个安装包都附带 minisign 签名（`.minisig`），另有一份 `SHA256SUMS`。

装好之后不用再手动下载：ShellRS 会在后台检查、下载并校验新版本，标题栏右上角出现提示后点「重启并安装」；不点的话，退出时也会自动装好。直接在 DMG 里运行、没放进「应用程序」文件夹、在 Linux 上不是用 AppImage 运行，或者 Windows 便携版所在的文件夹不能写入时，无法自动更新。

## 快速上手

1. 点标题栏的「新建主机…」（⌘N，Windows / Linux 上 Ctrl+N），填写地址、端口和认证方式，密码用应用锁密码加密保存。
2. 双击主机打开终端。第一次连接时会请你确认主机密钥。
3. 终端连上后，点标签栏右侧的「上传」或「下载文件…」传文件。
4. 标题栏「ShellRS」后面的三个图标切换侧栏：主机、端口转发、凭据。
5. ⌘T（Ctrl+T）打开本地终端。把标签拖到中间区域的上、下、左、右边缘，就能分栏。

## 从源码构建

需要先装 [rustup](https://rustup.rs)。仓库里的 `rust-toolchain.toml` 固定了 Rust 1.99.1，第一次构建时会自动安装。

```sh
git clone https://github.com/dream10201/shell-rs.git
cd shell-rs
cargo run
```

首次构建要编译整个 GPUI 栈，需要一段时间。Linux 需要的系统依赖和测试方法见[开发与测试](docs/development.md)。

## 文档

- [使用手册](docs/manual.zh.md)：每个功能的详细说明和快捷键
- [开发与测试](docs/development.md)：构建、测试和数据目录
- [发布流程](docs/release.md)：打包、签名和在线更新的发布步骤
- [更新日志](CHANGELOG.zh.md)

## 路线图

以下功能尚未实现：

- Dock 布局在重启后保留
- 跳板和代理叠加使用、HTTPS 代理
- Windows 安装程序的代码签名

## 参与贡献

欢迎提交 [Issue](https://github.com/dream10201/shell-rs/issues) 和 Pull Request。架构说明和代码约定见 [CLAUDE.md](CLAUDE.md)。提交前请确认下面三条都通过：

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## 致谢

- [ShellRS](https://github.com/since2006/shell-rs)（since2006）：本项目 fork 自它
- [GPUI](https://github.com/zed-industries/zed)（Zed）和 [gpui-kit](https://gpui-kit.com)：界面框架
- [russh](https://github.com/Eugeny/russh)：SSH 协议
- [alacritty_terminal](https://crates.io/crates/alacritty_terminal) 和 [portable-pty](https://crates.io/crates/portable-pty)：终端模拟与本地 PTY
- [argon2](https://crates.io/crates/argon2)、[chacha20poly1305](https://crates.io/crates/chacha20poly1305)（RustCrypto）：本机数据的加密
- [Simple Icons](https://simpleicons.org)：系统徽章使用的 logo（CC0）
- [Termark](https://www.termark.app/) 和 Xshell：交互设计的参照

## 许可证

ShellRS 以 [GNU 通用公共许可证第 3 版（GPL-3.0）](LICENSE)发布。你可以自由使用、修改和再分发，包括用于商业用途；但分发修改后的版本时，必须同样以 GPL-3.0 公开完整的源代码。同步核心 [`crates/shellrs-sync`](crates/shellrs-sync) 以 [MIT 许可证](crates/shellrs-sync/LICENSE)发布。
