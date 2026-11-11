<p align="center">
  <img src="assets/logo/shellrs.svg" width="96" alt="ShellRS">
</p>

<h1 align="center">ShellRS</h1>

<p align="center">
  A high-performance, native, cross-platform SSH client written in Rust, with a GPU-rendered interface: Xshell-style host management, tabbed terminals, file uploads and downloads, and port forwarding, all in one window.
</p>

<p align="center">
  <a href="https://github.com/dream10201/shell-rs/releases"><img src="https://img.shields.io/github/v/release/dream10201/shell-rs?include_prereleases&label=release" alt="Release"></a>
  <a href="#license"><img src="https://img.shields.io/badge/license-GPL--3.0-blue" alt="License: GPL-3.0"></a>
  <img src="https://img.shields.io/badge/platform-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey" alt="Platform: macOS | Windows | Linux">
  <img src="https://img.shields.io/badge/rust-1.99-orange" alt="Rust 1.99">
  <a href="https://linux.do"><img src="https://img.shields.io/badge/LINUX-DO-FFB003.svg?logo=data:image/svg%2bxml;base64,DQo8c3ZnIHhtbG5zPSJodHRwOi8vd3d3LnczLm9yZy8yMDAwL3N2ZyIgd2lkdGg9IjEwMCIgaGVpZ2h0PSIxMDAiPjxwYXRoIGQ9Ik00Ni44Mi0uMDU1aDYuMjVxMjMuOTY5IDIuMDYyIDM4IDIxLjQyNmM1LjI1OCA3LjY3NiA4LjIxNSAxNi4xNTYgOC44NzUgMjUuNDV2Ni4yNXEtMi4wNjQgMjMuOTY4LTIxLjQzIDM4LTExLjUxMiA3Ljg4NS0yNS40NDUgOC44NzRoLTYuMjVxLTIzLjk3LTIuMDY0LTM4LjAwNC0yMS40M1EuOTcxIDY3LjA1Ni0uMDU0IDUzLjE4di02LjQ3M0MxLjM2MiAzMC43ODEgOC41MDMgMTguMTQ4IDIxLjM3IDguODE3IDI5LjA0NyAzLjU2MiAzNy41MjcuNjA0IDQ2LjgyMS0uMDU2IiBzdHlsZT0ic3Ryb2tlOm5vbmU7ZmlsbC1ydWxlOmV2ZW5vZGQ7ZmlsbDojZWNlY2VjO2ZpbGwtb3BhY2l0eToxIi8+PHBhdGggZD0iTTQ3LjI2NiAyLjk1N3EyMi41My0uNjUgMzcuNzc3IDE1LjczOGE0OS43IDQ5LjcgMCAwIDEgNi44NjcgMTAuMTU3cS00MS45NjQuMjIyLTgzLjkzIDAgOS43NS0xOC42MTYgMzAuMDI0LTI0LjM4N2E2MSA2MSAwIDAgMSA5LjI2Mi0xLjUwOCIgc3R5bGU9InN0cm9rZTpub25lO2ZpbGwtcnVsZTpldmVub2RkO2ZpbGw6IzE5MTkxOTtmaWxsLW9wYWNpdHk6MSIvPjxwYXRoIGQ9Ik03Ljk4IDcwLjkyNmMyNy45NzctLjAzNSA1NS45NTQgMCA4My45My4xMTNRODMuNDI2IDg3LjQ3MyA2Ni4xMyA5NC4wODZxLTE4LjgxIDYuNTQ0LTM2LjgzMi0xLjg5OC0xNC4yMDMtNy4wOS0yMS4zMTctMjEuMjYyIiBzdHlsZT0ic3Ryb2tlOm5vbmU7ZmlsbC1ydWxlOmV2ZW5vZGQ7ZmlsbDojZjlhZjAwO2ZpbGwtb3BhY2l0eToxIi8+PC9zdmc+" alt="LINUX DO"></a>
</p>

<p align="center"><b>English</b> | <a href="README.zh-CN.md">简体中文</a></p>

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/screenshots/en/main-dark.png">
  <img src="docs/screenshots/en/main-light.png" alt="ShellRS main window: a grouped host tree on the left, htop running in a remote terminal on the right, connection latency in the tab bar">
</picture>

## About

ShellRS brings Xshell-style host management and tabbed terminals, file uploads and downloads over a terminal’s connection, and SSH port forwarding together in one cross-platform app, which bastion hosts such as JumpServer can launch the way they launch Xshell. It is written in Rust, and its UI is built on GPUI, the GPU-accelerated UI framework of the Zed editor (through [gpui-kit](https://gpui-kit.com)). There is no Electron and no JVM.

Hosts, groups, credentials and forwarding rules live in a local SQLite database. Passwords and key passphrases go only into the system keychain (macOS Keychain, Windows Credential Manager, Linux Secret Service); the database never holds a secret.

## Features

**Hosts**

- Groups nest to any depth; drag to rearrange, type to search.
- The remote OS is detected on connect and shown as a badge in the host tree and on tabs (Linux distributions, the BSDs, macOS, Windows).
- Three ways to log in: password; no password (the server's `none` auth, then the SSH agent, then the default keys in `~/.ssh`); or a saved credential.
- Three ways to connect: directly, through a chain of SSH jump hosts, or through an HTTP or SOCKS5 proxy. Terminals, file transfers and port forwards all follow it.
- "Test connection" performs a real login and explains any failure.
- Besides SSH: Telnet hosts for switches, console servers and lab devices, with automatic login at the device's prompts and the same jump hosts and proxies; and serial ports of this computer, for console cables and debug ports.

**Terminal**

- Tabbed remote and local terminals (built on `alacritty_terminal`); full-screen programs like vim and htop work as expected.
- Each tab shows the SSH round-trip latency live.
- Find (smart case) and local clear; configurable font, size and line height.
- ZMODEM: `rz` and `sz` upload and download files right in the terminal, over SSH, Telnet or a serial line; drop files on a terminal to upload them.
- Drag tabs into side-by-side or stacked splits to watch several terminals together.

**Right sidebar tools**

- They follow the current remote terminal and read and run on that terminal's own connection, with no second login. ⌘⌥B (Ctrl+Alt+B elsewhere) shows or hides them.
- System monitor: system details, CPU (per core too), memory, network rates and disk use, refreshed every 2 seconds.
- Network connections, processes and services: search and filter, process details, ending processes, starting, stopping and restarting systemd services, switching them on at boot, and their logs; when not root, passwordless sudo is used. These three and the monitor are for Linux only.
- Command history reads `~/.bash_history` on the host, and snippets, shared by every host, can be sorted into categories; click one to type it into the terminal, or run it straight away.
- tmux: the host's tmux sessions, with the one the terminal is in marked; start one, attach the terminal to one or detach it, rename and end them.

**File transfer**

- Once an SSH terminal is connected, "Upload files…" and "Download files…" at the right of its tab bar move files and folders over that terminal's own connection: no second login, and jump hosts and proxies work as they do for the terminal.
- Neither SFTP nor `scp`: only `cat`, `wc`, `mkdir`, `mv` and `find` in the host's shell, so a server without the SFTP subsystem works too.
- An upload goes to a temporary file first and replaces the old one only once it has arrived whole, so a dropped connection leaves no half file. Progress shows in the status bar, and you can cancel.

**Opening from a bastion host**

- Launched the way Xshell is: in the client of a bastion host such as JumpServer, set ShellRS as the SSH client. An `ssh://` link opens a terminal.
- Understands Xshell's `-url` and `-newtab`. When ShellRS is already running, the link opens there.
- What opens is an unsaved external connection: it stays out of the host list, keeps the link's password in memory only and goes away with its tabs. Each connection uses a single channel, so bastion hosts that allow no more still work. See the [user manual](docs/manual.md#opening-from-a-bastion-host-external-connections).

**Port forwarding**

- Local (`-L`), remote (`-R`) and dynamic (`-D`, SOCKS5 / SOCKS4) forwards.
- The rule dialog draws a live diagram and a one-line explanation of where connections enter and where they go.
- Every rule keeps its own SSH connection and reconnects three times after a drop; rules can start with ShellRS.

**Credentials**

- Password, key and SSH agent credentials that many hosts can share. Change a credential and every host using it follows.
- Keys can reference a file on disk, be pasted in, or be generated on the spot (Ed25519 or RSA 4096), with the public key one click away.

**Security and updates**

- Secrets stay in the system keychain. Host keys are recorded in ShellRS's own `known_hosts`, never in `~/.ssh`.
- In-app updates: the update manifest is minisign-signed and each package is checked against its SHA-256. Updates download in the background and install on restart or quit. There are stable and beta channels, and an update check sends only the version, OS and CPU architecture.

## Screenshots

<p align="center">
  <img src="docs/screenshots/en/tools.png" alt="Three right sidebar tools side by side: Monitor, Processes and Services"><br>
  <em>Right sidebar tools: Monitor, Processes and Services</em>
</p>

<p align="center">
  <img src="docs/screenshots/en/forward.png" alt="Edit port forward dialog with the diagram and explanation of a local forward, and two running rules in the forward list"><br>
  <em>Port forwarding: the diagram shows where each connection goes</em>
</p>

## Download

Download from GitHub [Releases](https://github.com/dream10201/shell-rs/releases). Betas are marked Pre-release there; to get new features early, switch the update channel to Beta in Settings › About.

| System | Package | Notes |
| --- | --- | --- |
| macOS 11+ (Apple Silicon) | `ShellRS-<version>-macos-aarch64.dmg` | Signed and notarized. Open the DMG and drag ShellRS into Applications |
| macOS 11+ (Intel) | `ShellRS-<version>-macos-x86_64.dmg` | Signed and notarized. Open the DMG and drag ShellRS into Applications |
| Windows x64 | `ShellRS-<version>-windows-x86_64-setup.exe` | Installs per user, no administrator rights needed. The installer is not signed yet; if SmartScreen stops it, choose "More info › Run anyway" |
| Windows x64 (portable) | `ShellRS-<version>-windows-x86_64-portable.zip` | Unzip into a folder you can write to and run `shellrs.exe`; its data stays in the `data` folder beside it. See [Windows portable](docs/manual.md#windows-portable) |
| Linux x64 | `ShellRS-<version>-linux-x86_64.AppImage` | `chmod +x` and run. Needs glibc 2.31 or newer (Ubuntu 20.04, Debian 11 and later) |

Every package comes with a minisign signature (`.minisig`), and each release has a `SHA256SUMS` file.

Once installed, ShellRS keeps itself up to date: it checks, downloads and verifies new versions in the background, then shows an icon in the top-right corner of the title bar; click it and choose "Restart to update", or just quit and the update is installed on the way out. Automatic updates are unavailable when ShellRS runs from inside the DMG or outside the Applications folder, on Linux when it isn't run as the AppImage, or when the Windows portable copy's folder can't be written to.

## Getting started

1. Click "New host…" in the title bar (⌘N, or Ctrl+N on Windows and Linux) and fill in the address, port and login. Passwords go into the system keychain.
2. Double-click a host to open a terminal. On the first connection you're asked to confirm the host key.
3. Once a terminal is connected, click "Upload files…" or "Download files…" at the right of its tab bar to move files.
4. The three icons after "ShellRS" in the title bar switch the sidebar between hosts, port forwards and credentials.
5. ⌘T (Ctrl+T) opens a local terminal. Drag a tab to the top, bottom, left or right edge of the tab area to split it.

## Building from source

Install [rustup](https://rustup.rs). The repository's `rust-toolchain.toml` pins Rust 1.99.1, which rustup installs on the first build.

```sh
git clone https://github.com/dream10201/shell-rs.git
cd shell-rs
cargo run
```

The first build compiles the whole GPUI stack and takes a while. See [development notes](docs/development.md) (in Chinese) for the Linux system packages and how to run the tests.

## Documentation

Apart from the user manual and the changelog, the documentation is in Chinese for now:

- [User manual](docs/manual.md): every feature in detail, with shortcuts
- [Development](docs/development.md): building, testing, the data directory
- [Releasing](docs/release.md): packaging, signing and publishing updates
- [Changelog](CHANGELOG.md)

## Roadmap

Not implemented yet:

- Keeping the Dock layout across restarts
- Combining a jump host chain with a proxy, and HTTPS proxies
- Code signing for the Windows installer

## Contributing

Issues and pull requests are welcome on [GitHub](https://github.com/dream10201/shell-rs/issues). Architecture notes and code conventions are in [CLAUDE.md](CLAUDE.md) (in Chinese). Before sending a change, make sure these pass:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

## Acknowledgements

- [ShellRS](https://github.com/since2006/shell-rs) by since2006: the project this one is forked from
- [GPUI](https://github.com/zed-industries/zed) (Zed) and [gpui-kit](https://gpui-kit.com): the UI framework
- [russh](https://github.com/Eugeny/russh): SSH
- [alacritty_terminal](https://crates.io/crates/alacritty_terminal) and [portable-pty](https://crates.io/crates/portable-pty): terminal emulation and local PTYs
- [keyring](https://crates.io/crates/keyring): the system keychain
- [Simple Icons](https://simpleicons.org): the OS logos on host badges (CC0)
- [Termark](https://www.termark.app/) and Xshell: the interaction designs ShellRS follows

## License

ShellRS is licensed under the [GNU General Public License v3.0](LICENSE). You may use, modify and redistribute it, commercially or not, but any modified version you distribute must come with its complete source code under the GPL-3.0 as well. The sync core in [`crates/shellrs-sync`](crates/shellrs-sync) is licensed under the [MIT License](crates/shellrs-sync/LICENSE).
