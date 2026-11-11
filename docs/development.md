# 开发与测试

[返回 README](../README.zh-CN.md)

## 构建

装好 [rustup](https://rustup.rs) 即可：`rust-toolchain.toml` 固定了 Rust 1.99.1，第一次构建时 rustup 会自动装上它，不改动机器上默认的工具链。首次构建要编译整个 GPUI 栈，需要一段时间。

Linux 需要先装这些开发包（Debian / Ubuntu，与发布构建用的一致）：

```sh
sudo apt-get install -y cmake clang pkg-config libfontconfig-dev \
  libfreetype-dev libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev \
  libx11-xcb-dev libxcb1-dev libvulkan-dev libdbus-1-dev libssl-dev
```

Windows 只在 CI（`.github/workflows/windows.yml`）上编译和测试。C 运行库静态链接进 exe（`.cargo/config.toml`），程序不依赖 VC++ 运行库，LTSC 版 Windows 也能直接启动；`scripts/windows-runtime.ps1` 在 CI 里检查。

## 运行和验证

工具链固定 Rust 1.99.1；保持仓库锁定的 GPUI 与 russh 版本。

```sh
cargo run
cargo test
cargo test -p shellrs-sync   # 只跑同步核心，不编 GPUI
cargo test -p shellrs-ssh    # 只跑 SSH 核心，不编 GPUI
cargo test -p shellrs-hosts  # 只跑主机核心，不编 GPUI
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

仓库是 Cargo workspace：根包是桌面应用，`crates/shellrs-sync` 是同步核心（MIT 许可，文件格式见 `crates/shellrs-sync/FORMAT.md`），`crates/shellrs-ssh` 是 SSH 核心（MIT 许可，登录、通道和经主机 shell 的文件传输），`crates/shellrs-hosts` 是主机核心（MIT 许可，主机、凭据等的模型和数据库，以及同步的一轮）。`cargo` 命令覆盖全部四个。

数据目录依次是：`SHELLRS_DATA_DIR`；便携版（程序旁边有 `data` 文件夹：Windows 的 `shellrs.exe`、Linux 的 `.AppImage`、macOS 的 `ShellRS.app` 旁；Windows 上旁边还不能有安装程序留下的 `unins000.exe`）的 `data`；系统应用数据目录中的 `shellrs`。保存的东西都在该目录的 `vault/` 下，用应用锁密码加密（`src/vault/`）：`vault.json` 是 Argon2id 参数、盐和包裹后的数据密钥，不加密；其余每个文件是 `SRSV\x01` 文件头、nonce 和 XChaCha20-Poly1305 密文，绑定文件名。主机、端口转发规则和凭据（不含密码）保存在 `shellrs.db`（数据库在内存里跑，整库存成这个文件），主机信任记录保存在 `known_hosts`，凭据的私钥保存在 `keys/` 下，设置在 `settings.json`，密码和私钥口令在 `secrets`（按账户名存的 JSON；账户名是主机端点的 `password:<用户>@<地址>:<端口>`、私钥的 `passphrase:<路径>`、密码凭据的 `credential:<随机 ID>` 和代理的 `proxy:<用户>@<代理地址>:<端口>`）。不加密的还有数据目录下的 `window.json`。这些文件都经 `shellrs_ssh::files::{read, write}` 读写；测试里没装保险库，文件是明文。

文件传输（`crates/shellrs-ssh/src/transfer.rs`）不用 SFTP，也不用 `scp`：每一步是在终端自己的连接上另开通道执行的一行 `sh -c '…'`，文件内容经 `cat` 进出。它的测试用本机的 `sh` 真跑这些命令（上传、下载、文件夹、权限位、取消、主机上没有的路径）；`src/ssh/transport.rs` 里另有一条测试经进程内的 SSH 服务器把同样的命令交给本机的 `sh`，走一遍真实的通道。只监听 `127.0.0.1` 的随机端口，使用测试凭据；限制本地套接字的沙箱会跳过该部分。

`src/forward/protocol_tests.rs` 用进程内的 SSH 服务器和本机的回显服务测试真实的转发工作线程：三种转发的往返、目标不可达、服务器禁止转发、端口被占用、空闲断线后重连、重试耗尽和停止后的清理，同样只监听 `127.0.0.1` 的随机端口。

`tests/ui/` 里的 UI 测试驱动真实 `Workspace`，通过 `Workspace::new_with_services` 注入终端和端口转发服务，不连接用户服务器，也不碰用户的数据目录；在线升级的测试给 `workspace.updater()` 注入假的更新服务器和安装器，不联网、不改任何安装。

开发构建（`cargo run`）不检查更新。要在本机走一遍更新流程，用 debug 构建加环境变量 `SHELLRS_UPDATE_CHANNEL=beta` 编译，运行时用 `SHELLRS_UPDATE_URL=http://127.0.0.1:8000/{channel}.json` 指向本地的静态服务器（`python3 -m http.server`），清单用 `packaging/make-manifest.sh` 和自己生成的测试密钥签名，测试公钥放在环境变量 `SHELLRS_UPDATE_PUBLIC_KEY` 里。release 构建不认这两个环境变量。

## README 截图

`docs/screenshots/` 里的截图来自真实窗口（macOS 的 `screencapture -o -l <窗口号>`，2 倍分辨率）。截图时不要用自己的数据：把数据目录复制一份，用 `SHELLRS_DATA_DIR` 指向这份副本启动 ShellRS（应用锁密码和原来一样），再改掉分组、主机的名称和地址；保存的密码按「用户@地址:端口」记，所以副本里没改地址的主机照样能真实连接。截好后检查状态栏、对话框和终端输出里有没有真实的 IP、主机名或业务信息，有就打码。
