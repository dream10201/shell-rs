# 发布

[返回 README](../README.zh-CN.md)

在 `CHANGELOG.md`（英文）和 `CHANGELOG.zh.md`（中文）里各写好 `## [x.y.z]` 一节，把 `Cargo.toml` 的版本改成 `x.y.z`，提交后推送 tag `vx.y.z`。`.github/workflows/release.yml` 依次：

1. 核对 tag 与 `Cargo.toml` 一致，两份更新日志都有这一节，从英文版截出 GitHub Release 的说明，确认代码里有发布公钥；
2. 构建并打包：macOS 按 `aarch64`（Apple Silicon）和 `x86_64`（Intel）分别构建、签名、公证、staple，各自产出升级用的 `.app.zip` 和首次安装用的 `.dmg`；Windows 用 Inno Setup 打安装程序，另打便携版的 zip（根上是 `shellrs.exe` 和空的 `data` 文件夹，用 7-Zip 打包，打完核对正好是这两项），它既是首次下载的包、也是便携版升级用的包；Linux 打 AppImage；
3. 在 `release` 环境里：给每个包签名，建 GitHub Release，**最后**才替换更新清单：`update` 分支上的 `v1/<通道>.json`，客户端从 `raw.githubusercontent.com/dream10201/shell-rs/update/v1/<通道>.json` 读它。清单里每个包的下载地址就是 GitHub Release 的（仓库是公开的，下载不用登录）。

版本号带 `-` 的（如 `0.3.0-beta.1`）只发到 beta 通道；正式版同时成为 beta 通道的最新版。在 Actions 里手动运行这个 workflow 只构建打包、不发布，用来演练。客户端只接受比自己新的版本，所以发错的版本撤不回来：把清单改回上一版能阻止更多人升级，修复要发新的补丁版。

一次性准备：

- **更新签名密钥**：`minisign -G -p shellrs-update.pub -s shellrs-update.key` 生成两对（一对日常用，一对离线保存备用），把两个公钥（`.pub` 文件的第二行）填进 `src/update/build_info.rs` 的 `TRUSTED_KEYS`。私钥和口令只放进 GitHub 的 `release` 环境，另外离线备份；丢了私钥，已发出去的 ShellRS 就再也收不到更新。换钥匙时先发一版带上新公钥，再换签名用的私钥。
- **Apple**（可选）：Developer ID Application 证书（导出 .p12）和 notarytool 用的 App Store Connect API Key。没有这些 secrets 时，macOS 的包只做 ad-hoc 签名、不公证：第一次打开要在「系统设置 › 隐私与安全性」里放行。`packaging/macos/Info.plist.in` 里的 bundle id `com.shellrs.ShellRS` 和签名的 Team ID 一经发布就不能再改：在线升级按 Team ID 核对新版本的签名。
- **图标**：`assets/logo/` 里的 `shellrs.icns`（macOS）、`shellrs.png`（1024×1024，Linux，macOS 没有 icns 时也由它生成）和 `shellrs.ico`（Windows 安装程序）。
- **GitHub secrets**：`release` 环境里的 `MINISIGN_SECRET_KEY`（私钥文件的全文）、`MINISIGN_PASSWORD`（私钥有口令时）；要发布前人工批准，就给这个环境设审批人。可选的仓库级 secrets：`APPLE_CERTIFICATE_P12`（base64）、`APPLE_CERTIFICATE_PASSWORD`、`APPLE_SIGNING_IDENTITY`、`APPLE_TEAM_ID`、`APPLE_API_KEY_P8`（base64）、`APPLE_API_KEY_ID`、`APPLE_API_ISSUER`。

清单格式见 `packaging/manifest.example.json`，由 `packaging/make-manifest.sh` 生成：一个文件里是清单原文和它的 minisign 签名，签名的 trusted comment 写明通道和版本。格式只增加字段、不改已有的；已发出去的 ShellRS 一直按 `update` 分支的 `v1/` 这个地址检查。

macOS 产物名分别为 `ShellRS-<版本>-macos-aarch64.{app.zip,dmg}` 和 `ShellRS-<版本>-macos-x86_64.{app.zip,dmg}`。更新清单保留原有的两个平台键，分别记录对应架构包的地址、大小和 SHA-256；已有 universal 客户端按当前运行的架构升级。Apple Silicon 上通过 Rosetta 运行 Intel 版时仍会收到 Intel 包，需要手动安装 ARM 版才能切换。

Windows 便携版的产物名为 `ShellRS-<版本>-windows-x86_64-portable.zip`，清单里是新的平台键 `windows-x86_64-portable`（`assets` 和 `installers` 都指向这个 zip；旧客户端不认这个键，照旧用 `windows-x86_64` 的安装程序）。这个键一经发布就不能再改。
