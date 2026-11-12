# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## 项目是什么

ShellRS（crate 与二进制都叫 `shellrs`）是 Xshell 式的 SSH 主机管理工具，基于 `gpui-kit` 0.9.1（GPUI + gpui-base + gpui-component）。界面有简体中文和英文（中文是源语言），标识符用英文。功能说明见使用手册 `docs/manual.zh.md` / `docs/manual.md`；开源首页 `README.md`（英文）/ `README.zh-CN.md`；截图在 `docs/screenshots/`。本仓库（`dream10201/shell-rs`）fork 自 `since2006/shell-rs`，发布、升级都只用本仓库的 GitHub。

- **术语**：保存的一项叫「主机」（量词「台」），代码里叫 host（`Host`、`HostStore`、`hosts` 表）；以前叫「会话」/ `Session`，别再用。IP / 域名字段叫「地址」。SSH 协议层的 session 照旧叫 session。
- **两种不保存的连接**：「外部连接」是堡垒机用 `ssh://` / `-url` 拉起 ShellRS，只开一个标签；「临时连接」从标题栏打开，功能齐全。代码里统称 temporary。
- **全是真的**：SQLite（数据目录默认在系统数据目录，`SHELLRS_DATA_DIR` 可覆盖，便携版在程序旁的 `data`；ShellRS 存的东西都在数据目录的 `vault/` 里，用应用锁密码加密）、本地终端、SSH（`russh`）、Telnet（自己写）、串口（`serial2`）、文件传输、端口转发、凭据、右侧栏工具、在线升级。密码和口令只进保险库里加密的 `secrets` 文件，不用系统钥匙串，**数据库里永远不出现秘密**。

做 UI 之前先读 `gpui-kit` 与 `gpui-kit-design-guides` 两个技能，里面的 Coding Guides 和 Design Guides 在本仓库是规范。不要凭记忆臆造 gpui-kit API，以 `~/.cargo/registry/src/*/gpui-component-0.9.1/`、`gpui-base-0.9.1/` 的源码为准。

## 已定的产品决定

用户定过的事，不要改回去。细节以使用手册和代码为准。

- **界面语言**：跟随系统（默认）/ 简体中文 / English，即时生效；用 gpui-kit 自带的 `rust-i18n`。
- **没有 SFTP**：1.0.2 起整个去掉了（SFTP 标签、右侧栏「文件」和目录跟随、编辑器、预览、查找文件、传输队列、`sftp://` 链接），手机端的 crate 也一并删了，不要加回来（用户定的）。
- **文件传输**：只做上传和下载，入口是 SSH 终端标签栏右侧的两个按钮和右键菜单；「上传」按钮点开是「上传文件…」「上传文件夹…」两项（系统的选择框只有 macOS 能同时选文件和文件夹，用户定的），右键菜单里直接列这两项；走终端自己的连接、另开 exec 通道，用主机 shell 的 `cat`、`wc`、`mkdir`、`mv`、`find`（不用 SFTP 也不用 `scp`，用户定的）。上传先写 `.shellrs-part`、大小对了才改名替换；没有续传、没有文件列表；进度在状态栏、可取消，结束弹通知。只许一个通道的堡垒机上用不了，用 ZMODEM。
- **标签各自记着打开时的主机**（同 Xshell、SecureCRT）：编辑、删除主机或分组都不碰已打开的标签（不重连、不改标题、不关）；没有主机级的连接状态，状态栏显示当前标签自己的连接。
- **主机树**：搜索匹配主机和分组名；分组可任意嵌套；删除分组连同子分组和主机。首次启动是空库。
- **导入主机**（issue #1）：从 `~/.ssh/config` 和 `~/.ssh/known_hosts` 导入，入口在主机侧栏的「…」菜单和空白处右键菜单（用得少，不占工具栏）。先预览再导入，只追加、不改已有主机；每行可改名称、分组、用户名和认证，状态写「新增」/「已存在」。ProxyJump、ProxyCommand 不导入；known_hosts 用户名填 root；非默认私钥建密钥凭据（不带用户名，一把私钥一条，各主机用自己的用户名），私钥复制进 `keys/`（再导入同一个文件认得出那份副本），其余默认「无密码」。
- **主机对话框**：协议 SSH / Telnet / 串口，编辑时也能换（被当跳板或有转发经由的 SSH 主机除外）。认证三选一：密码（留空就每次问）/ 使用凭据（下面有用户名，留空用凭据的，填了用填的）/ 无密码（试 Agent 和默认私钥，不问密码）；主机自己没有私钥文件，要用私钥就建凭据。「测试连接」是真登录。连接方式：直接 / SSH 跳板（只走列出的链）/ HTTP 或 SOCKS5 代理，跳板和代理不叠加。
- **凭据**：侧栏第三个列表（密码 / 密钥 / SSH Agent）；用户名可选：留空由各主机自己填，一把密钥可以给多个用户用；改凭据的用户名只带着用它用户名的主机变，清空时这些主机保留原用户名（用户定的）。密钥来源本机文件 / 粘贴 / 生成，私钥一律存成 `vault/keys/` 下的文件（同保险库里别的文件一样加密）：粘贴和生成的直接写进去，选本机文件的复制一份进去，不出勾选框；原文件只读这一次、从不改动，用户改了原文件、连不上时自己重新选（用户定的）。同步只同步 `keys/` 里的私钥；升级前只记路径的旧凭据不动。删除在用的凭据要确认，那些主机改回自己登录。
- **串口主机**：默认 115200 8N1；连接是普通远程终端标签，Unix 上独占；没有文件传输、跳板、转发，右侧栏只有命令片段，有「发送 Break」。
- **Telnet 主机**：协议自己写，不引第三方库。可选自动登录（手动或密码凭据）；连接方式同 SSH；服务器不回显时在本地编辑一行；不做 `telnet://` 外部连接；右侧栏只有命令片段。
- **端口转发**：侧栏第二个列表，每条规则经由一台保存的主机、自己建连接，和主机的标签无关。「创建」只保存不启动，可设随 ShellRS 自动开启；断线自动重连三次，不弹问答。
- **临时连接与外部连接**：临时连接的密码只在内存；外部连接只认命令行参数，不注册系统链接，打开时收起左侧栏。
- **没有外部 CLI**：1.0.1 起整个去掉了（`shellrs` 命令、`shellrs-cli`、设置页、Agent Skills、`exec --terminal`），不要加回来（用户定的）。
- **没有匿名使用统计，也没有 Docker、Kubernetes、GPU/NPU 工具**：1.0.5 起整个去掉了匿名使用统计（Aptabase）和右侧栏的 Docker、Kubernetes、GPU/NPU 三个工具，不要加回来（用户定的）。
- **本地终端**：Shell 设置是输入框加常用 shell 下拉；留空自动检测（Windows 优先 PowerShell 7）。
- **开始页**：没有标签时显示「最近连接」和「快速连接」。
- **右侧栏**：只跟着终端出现，工作区级一份；开没开、开哪个工具按主机记（存在主机表里，切标签也跟着变，issue #46），没开过的主机收起，本地终端共用一份（存设置里）；分屏时跟最近在前的终端，本地远程都算（用户定的）。本地终端旁只有「项目」和「命令片段」（「项目」在前；片段打进本地终端，Windows 上不清行，同 Windows 主机）；远程终端旁七个工具：命令片段、历史命令、tmux、系统服务、进程管理、网络连接、系统监控；在「设置 › 终端 › 侧栏功能」里切换显示，只在设置里改。按主机系统过滤工具。工具在终端自己的连接上执行命令；危险操作先确认，非 root 用 `sudo -n`；列表默认不自动刷新，定时读的只在显示时读。
- **命令片段**：分类像主机分组一样任意嵌套、拖动排序（片段拖到片段旁或分类上，分类拖到中间成为子分类，拖到空白处回最外层），没有分类的片段在最外层、分类后面，不再有「未分类」；每行只显示名称，悬停在行左边显示命令；同一层级的分类不能重名（同主机分组）；顺序和嵌套跟着同步（用户定的）。
- **项目**（本地终端）：右侧栏「项目」像主机一样保存本地终端，可以不分组；分组只有名字、不嵌套，和磁盘目录无关，分组和终端都能拖动排序，新建按钮在标题栏；删除终端连标签一起关，开着的才确认（用户试过「项目=目录+启动命令」和「目录列表」两版，都不要）。分组的「+」直接开一个终端、不填东西；关掉标签终端还在（变灰），点一下重开。每个终端自动记住 shell 最后所在的文件夹（Windows 读不到就回主目录），退出时开着的（顺序、在前的）随时写库，下次启动在原文件夹重开；「保留终端内容」（默认开）关标签和退出时记下最后 1000 行输出，重开时先显示。列表和标签上的名字跟 shell 和程序设的标题走（关标签、重启后记着最后一个），手动改过名的固定用那个名字（用户定的）。⌘T 的本地终端和分屏不恢复。
- **同步**（核心在 `crates/shellrs-sync`，方案见设计文档）：「设置 › 云同步」选一个文件夹（网盘同步的文件夹）、连接 WebDAV（坚果云、Nextcloud 等，地址是服务器上的文件夹）或 S3（AWS、R2、MinIO、OSS、COS 等兼容的，桶要用户先建好），没有同步就创建（同步密码至少 12 个字符）、已有就加入；第一次加入时的去重预览放下一轮（用户定的）。WebDAV 密码或 S3 的 Secret Key 只进保险库的 `secrets`（账户 `sync:login`），被拒时停下请用户重新登录，不弹框问。密码、口令和 `keys/` 里的私钥一律跟着同步，不设开关；对安全有要求的人不开同步就是（用户定的）。串口主机不同步，只留在本机（串口号是这台电脑的）。启动、窗口回到前台（每分钟最多一次）、改动后停 5 秒、「立即同步」、退出前（最多等 3 秒）各同步一次，不定时轮询；出错只写在设置页，要用户处理的（别处改了密码、被重置、被移出、一次删很多）才弹通知，自动同步等用户处理完。「更换同步密码」总是连数据密钥一起换，只有这一个入口（用户定的）。关闭同步时本机数据不动。
- **在线升级**：后台检查并下载，校验完才在标题栏亮按钮；对话框只有「查看更新内容」（打开本仓库 GitHub 上的更新日志）和「重启并安装」；不点也在退出时装好。检查只发 User-Agent。
- **撰写窗格**：中间区底部一个（Xshell 式），发给在前的终端（本地远程都算，同右侧栏），Enter 发送、Shift+Enter 换行；快捷键和终端右键菜单开关，开没开记在设置里。
- **命令广播**：标签图标前出现复选框，勾满两个自动排成分屏，撰写窗格改为发给全部（广播时它就是命令栏）；命令原样发出、不先清行；同步按键默认关。
- **便携版**（各系统都有）：程序旁有 `data` 文件夹才算便携版（Linux、macOS 上要是空的或 ShellRS 用过的，别人的 `data` 不占），Windows 在 `shellrs.exe` 旁（安装程序装的那份不算）、Linux 在 `.AppImage` 旁、macOS 在 `ShellRS.app` 旁；全部数据和下载的升级包都在这个文件夹里，整个文件夹拿到别的电脑用同一个密码就能开，删掉就什么都不留。Windows 的 zip 既是首次下载也是升级包。
- **软件渲染**：CPU 渲染时终端少画几帧、光标不闪、关动画。
- **窗口**：默认记住大小和位置。左右侧栏拖动后的宽度记在设置里、下次启动照旧（左默认 280 最窄 180，右默认 320 最窄 220），「设置 › 应用 › 窗口 › 侧栏宽度」恢复默认（用户定的）。macOS 菜单栏只有「ShellRS」和「窗口」两个菜单，同其他 Mac 应用（issue #88）；⌘H、⌥⌘H、⌘M 是固定键，不进快捷键设置，也不能设给别的功能；不做「编辑」菜单（终端的 ⌘C、⌘V 有自己的规则）。
- **终端交互**：选中复制默认关；右键默认显示菜单。远程终端断开后按 Enter 在原标签重连，按 Space 关闭标签页。
- **标签连接状态**：标签图标彩色就是在线，连接中、断开时变灰（不用小圆点）。只在标签上变灰。
- **主机图标一律彩色**：没有品牌色的按地址从固定调色板取色，跨版本不变；绝不能是灰（灰留给离线）。
- **ZMODEM（rz / sz）**：用 `zmodem2`（钉死版本），自动识别、不设开关；进度写在终端里；可以拖文件到终端上传。不做 XMODEM / YMODEM。
- **没有终端通知和系统通知**：程序请求的通知（OSC 9 / 777）、响铃提醒、关键字高亮规则的「通知」和系统通知整个去掉了，不写注册表（Windows 上系统通知要登记 `HKCU\Software\Classes\AppUserModelId`），不要加回来（用户定的）；窗口里的提示（操作结果、出错）照旧。
- **字号**：界面字号在「外观」里，默认 16、Windows 14（对齐系统 9pt 正文）；⌘= / ⌘- / ⌘0 在终端里调终端字号，在终端外调界面字号。
- **主题**：浅色、深色各选一套；「界面跟随主题」默认关，界面是中性灰、只有终端配色（用户试过，别改回去）。默认深色用炭灰，不用近黑。只有内置主题，只收宽松许可的配色。
- **应用锁**：就是加密，必须设密码、关不掉（用户定的）。首次启动先设应用锁密码（至少 8 个字符，输两遍，「开始使用」），每次启动和锁定后都要输入；忘了密码数据就再也打不开。ShellRS 存的东西都封在数据目录的 `vault/` 里：数据库 `shellrs.db`、`settings.json`、`known_hosts`、`keys/`、`terminal-output/`、`compose-history.json`、`secrets`（密码、口令、代理密码、同步的密钥和登录）；不加密的只有 `vault/vault.json`（Argon2id 参数、盐、nonce、包裹后的数据密钥，没有密码）和 `window.json`（输密码前就要用）。Argon2id（256 MiB、3 轮、4 路、32 字节随机盐）把密码变成包裹密钥，包裹随机的 256 位数据密钥；每个文件用数据密钥经 XChaCha20-Poly1305 加密、绑定文件名。改密码只重新包裹数据密钥，不重写文件。完全不用系统钥匙串。不迁移：1.0.5 之前的明文数据和钥匙串里的密码不读也不删，从空库开始（用户定的）。锁定就是忘掉内存里的密钥：封着的读不了也写不了；开着的终端和端口转发照常跑（用户定的），锁定期间不新建连接、不自动重连、不同步、不保存，期间的改动解锁后写下。输密码之前窗口用系统语言和默认主题（设置也封着）。「设置 › 应用 › 应用锁」只有「立即锁定」和「修改密码…」。「多久未操作自动锁定」和「电脑锁屏时锁定」（默认关）都由工作区每秒一次的循环查，锁屏状态经 `ScreenState`（macOS 会话字典、Windows WTS、Linux logind 的 `LockedHint`）。
- **键盘快捷键**：设置里单独一类，只收主要功能；冲突就拒绝。终端里只按 Ctrl 加字母的通用快捷键让给 shell，所以「关闭标签」在 Windows / Linux 默认 Ctrl+Shift+W（同 Windows Terminal；不把 Ctrl+W 从 shell 抢过来，用户定的）；「显示或隐藏 ShellRS」是系统级快捷键，默认不设键。
- **关键字高亮**：设置里单独一类；规则是正则、颜色任意；总开关默认关，预置三条示例。

## 常用命令

```bash
cargo run                                   # 启动应用（发布版开着时只会激活它：先 ⌘Q 或设 SHELLRS_DATA_DIR）
SHELLRS_SIGN_IDENTITY="Apple Development: …" cargo run   # macOS：先用固定证书签名（scripts/macos-run.sh）
cargo test                                  # 单元测试 + tests/ui/（无头窗口 UI 集成测试）
cargo test --lib                            # 只跑单元测试
cargo test --test ui new_host -- --nocapture   # 按名称片段跑单个 UI 测试
cargo test -p shellrs-sync                  # 只跑同步核心（不编 GPUI，快）
cargo test -p shellrs-ssh                   # 只跑 SSH 核心（不编 GPUI）；文件传输的测试用本机的 sh 真跑命令
cargo test -p shellrs-hosts                 # 只跑主机核心（数据库、HostData、同步一轮；不编 GPUI，快）
cargo test -p shellrs-sync webdav -- --include-ignored  # WebDAV 后端对本机 Apache（macOS 自带；Debian 装 apache2）；设 SHELLRS_TEST_WEBDAV_URL/_USER/_PASSWORD 改测别的服务器
cargo test -p shellrs-sync s3 -- --include-ignored      # S3 后端对 SHELLRS_TEST_S3_ENDPOINT/_ACCESS_KEY/_SECRET_KEY 指的服务；不设就跳过
cargo test --lib terminal::zmodem -- --include-ignored  # ZMODEM 对 Docker 里的 lrzsz
cargo clippy --all-targets -- -D warnings   # 必须无警告
cargo fmt --check
```

工具链由 `rust-toolchain.toml` 固定；`Cargo.lock` 里的 `gpui-pre*` 和 `cc` 不要随手 `cargo update`。本终端的 `screencapture` 被拦，视觉检查靠 UI 测试或让用户看窗口。

## 架构

Cargo workspace，`cargo` 命令覆盖全部四个包：

- 根包是桌面应用 `shellrs`：单个二进制 crate，带 `src/lib.rs`，好让 `tests/ui/` 驱动生产环境的 `Workspace`。
- `crates/shellrs-sync` 是同步核心（MIT 许可）：不依赖 GPUI，也不依赖应用的类型；文件格式见它的 `FORMAT.md`。
- `crates/shellrs-ssh` 是 SSH 核心（MIT 许可）：登录（`SshConnector`、known_hosts、认证、跳板、代理）、连接上的通道（pty + shell、exec）、探测主机系统、经主机 shell 的文件传输（`transfer.rs`）。不依赖 GPUI 和应用的类型，不产出文案。
- `crates/shellrs-hosts` 是主机核心（MIT 许可）：主机、分组、凭据、端口转发规则、命令片段、项目终端的模型，`HostDatabase`（结构与迁移；在内存里跑，`image` / `from_image` 整库导出、载入）、`HostData`（内存为准、写穿 SQLite）、登录快照、`keys/` 里的私钥、秘密接口（`SecretStore`）和不落盘的实现；同步的导出和应用记录、一轮（`run_round`）、各任务的主体和建同步时的校验。不依赖 GPUI（模型的字符串用它单独的 `gpui-pre-shared-string`）和 rust-i18n，不产出文案。

模块按能力划分：功能模块不依赖 `workspace`（窗口壳），也不碰彼此的内部。

- `app/`：全部用户动作（`actions.rs`）、快捷键、数据目录路径、图标、退出守卫。
- `host/`：重新导出主机核心，加上应用自己的：GPUI 外壳 `HostStore`（`Deref` 到 `HostData`）、标签和错误的文案（`describe.rs`）、标签的连接状态、主机树和对话框、导入（`import/`）、`ssh://` 链接、快速连接。
- `connection.rs`：测试连接（问答类型在核心，这里按原来的名字重新导出）。`ssh/`：重新导出核心，加上应用自己的：错误和问答的文案（`describe.rs`）、远程终端传输、测试连接的实现。`telnet/`、`serial/`：各自的终端传输。
- `terminal/`：本地 PTY、驱动 `alacritty_terminal` 的引擎、`TerminalView`、关键字高亮、目录报告、ZMODEM。
- `transfer/`：文件传输的管理器（每个传输一个工作线程，定时收事件）、两个对话框和错误的文案；处理器在 `workspace/transfers.rs`。
- `forward/`：端口转发的运行和界面。`credential/`：凭据界面。`project/`：「项目」的树和对话框（模型在主机核心，打开、保存和恢复终端在 `workspace/projects.rs`）。
- `monitor/`、`netstat/`、`processes/`、`services/`、`tmux/`、`history/`、`snippets/`：右侧栏工具，各自是「命令 + 纯解析模型 + 面板」。
- `vault/`：保险库（`Vault`：`vault.json`、数据密钥、文件的加解密，装成 `shellrs_ssh::files` 的实现）和存秘密的 `VaultSecretStore`。`app_lock/`：应用锁（设密码、解锁、改密码的界面，空闲和锁屏的判断）。`secrets/`：只重新导出主机核心的秘密接口和实现。`settings/`：设置与设置页（「云同步」页在 `sync_page.rs`）。`instance/`：单实例和外部连接的链接。`update/`：在线升级。
- `sync/`：重新导出主机核心的同步，加上应用自己的：`SyncService`（什么时候同步，以及创建、加入、改密码、移除设备、关闭这些任务）、对话框和错误的文案（`describe.rs`）；一轮怎么走在核心的 `sync/round.rs`，导出、应用记录在核心的 `data/sync.rs`，处理器在 `workspace/sync.rs`。
- `workspace/`：窗口壳，持有 store、`DockArea` 和**全部动作处理器**，按领域拆文件。`shared/`：多个功能共用的展示片段。

## 关键约定与坑

### 状态与持久化

- **内存说了算，写穿。** `HostData` 改内存 → 写库；`HostStore` 调完再 `settle`：失败发 `PersistFailed`、有改动才 `notify`。写失败时内存照样改，绝不静默吞掉。设置同理写穿 `settings.json`。
- **秘密只进保险库的 `secrets`**，不进数据库，有守卫测试；`SecretStore` 是阻塞的，只在后台调用；秘密不放进派生了 `Debug` 的结构。ShellRS 保存的私钥在 `vault/keys/`；用户自己的私钥文件从不改动或删除。
- **数据库在内存里跑，整库存成一个加密文件**：`HostStore` 在 `settle` 后稍等一下（防抖）、锁定前、解锁后、退出时把整库写进 `vault/shellrs.db`；锁着时写不了，解锁后补写。
- **ShellRS 自己的文件一律经 `shellrs_ssh::files::{read, write}`**，不直接用 `std::fs`：装上保险库后 `vault/` 里的文件经它加解密，别处的（用户自己的私钥）原样读写；锁着时读写都失败（`PermissionDenied`）。例外只有不加密的 `vault.json` 和 `window.json`（`files::replace`）。
- **登录快照在 UI 线程解析**：工作线程拿到的是查好凭据和跳板的 `HostLogin`，不读 store。
- **对外只给 `PublicId`**，不给内部 id。
- **改表**：升 `SCHEMA_VERSION`、加一步迁移和「旧库升级后与新库比对」的测试；表都是 `STRICT`；库只往前升，迁移前先备份。
- **发布后不能改的标识**：更新清单地址和平台 key、凭据的 `keychain_id`、秘密的账户名（`SecretRef::account`，含同步密钥的 `sync:keyring` 和同步位置登录密钥（WebDAV 密码、S3 Secret Key）的 `sync:login`）、保险库的格式（`vault/vault.json` 的字段、加密文件的 `SRSV\x01` 文件头、`vault/` 里的文件名：文件名绑在密文上）、`Shortcut` 的 `id`、`ToolKind::key`、主题 key、`panel_name()`；同步的一切（`crates/shellrs-sync/FORMAT.md`：远程目录和文件名、`SRSY` 信封、`vault.json` 字段、记录的 kind 和字段名）。
- **一个数据目录只跑一个 ShellRS**（`src/instance`）：启动时先连数据目录里的 `instance.sock`（Windows 是命名管道），连得上就让已在运行的那个到前台、把链接交给它。Unix 套接字 0600 并核对 uid，Windows 用带 DACL 的命名管道。

### 多语言

- 文案在 `locales/<模块>.yml`，最外层写键的全名，下面 `zh-CN`、`en` 并排。代码用 `crate::i18n::t!` / `tn!`（带数量，英文分 `.one` / `.other`）。
- 键说用途，一句话一个键，不拼片段；插值写 `%{name}`。以 `%`、`{` 开头或含 `: ` 的值要加引号，否则那条文案会悄悄没了。
- 中文是源语言，改中文时英文一起改。实体里不存翻译好的字符串，render 时翻译，语言切换即时生效。
- 英文按 Design Guides：sentence case、单字符 `…`、按钮写结果，不写 Please / successfully。
- `src/i18n.rs` 的守卫测试检查两种语言齐全、键都被用到、`src/` 和 `crates/shellrs-ssh`、`crates/shellrs-hosts` 里测试以外没有中文字面量（语言名那种要留的加 `// i18n: keep`）。

用词表（新文案照此）：

| 中文 | English |
|---|---|
| 主机（台）/ 分组 / 地址 / 用户名 | host / group / address / username |
| 凭据 / 密码 / 密钥 / 私钥 / 口令 | credential / password / key / private key / passphrase |
| 认证：密码 / 使用凭据 / 无密码 | Password / Credential / No password |
| 连接方式：直接连接 / SSH 跳板 / 代理连接 | Direct / Jump hosts / Proxy |
| 临时连接… / 开始页「快速连接」 / 外部连接 / 最近连接 | Quick connect… / Find a host / external connection / Recent |
| 本地终端 / 远程终端 | local terminal / remote terminal |
| 连接 / 断开连接 / 重新连接 | Connect / Disconnect / Reconnect |
| 目录、文件夹 | folder |
| 本机 | this computer |
| 项目 / 新建本地终端 | Projects / New local terminal |
| 删除 | Delete（不用 Remove） |
| 正在读取… / 读取失败 | Loading… / Couldn’t load |

### GPUI / gpui-kit

- **一个命令一个处理器**：按钮、菜单、快捷键都派发 `app/actions.rs` 里的动作，由 `Workspace` 处理，不写临时闭包改状态（对话框内部自己的状态除外）。
- **焦点**：`dispatch_action` 只到焦点路径上的处理器，工作区里必须有东西持有焦点；不要聚焦工作区根节点；中间区面板在 `set_active(true)` 里收下焦点；对话框关掉后焦点为空，要经工作区的 focus handle 派发。焦点监听器里改状态要 `defer_in`。
- **后台线程不直接唤醒前台任务**，由 UI 定时轮询事件。
- **渲染回调不读取实体**（树行、表格单元），只传快照。窗口还没挂上 `Root` 时不能弹通知（用 `notify_once_open`）。
- **对话框**：都要 `.overlay_closable(false)`；表单错误用 `show_form_error`（工作区自己画的一层，盖在对话框上面；通知层在对话框遮罩下面），`on_close` 里 `dismiss_form_error`。对话框由 `Root` 画，不在工作区的元素树里：里面派发的动作到不了 `Workspace`，要经工作区的 focus handle 转；自己的页脚按钮从对话框里面的 focus handle 派发 `Confirm` / `Cancel`，不用 `window.dispatch_action`（焦点在下拉框上时到不了对话框）。应用锁上时关掉所有对话框和通知，解锁界面是最上面的一层。
- **右键菜单挂在容器上，不挂在行上**；滚动条挂在不滚动的外层上；`Select` 要放进有尺寸的容器；`uniform_list` 的行必须等高。
- **按上一帧的尺寸重排**：render 拿不到布局尺寸，用 `canvas` 在 prepaint 记下、变了再重画。
- **颜色**：用主题 token 和 rem，`px` 只用在 API 要 `Pixels` 的地方；要彩色图标就不能用 `Icon`（它是单色遮罩）。
- **快捷键改了要重建整个 keymap**（GPUI 删不了单条绑定），见 `app/shortcuts.rs`。
- **标签页全部可关**，关闭走 `DockArea::remove_panel`。左右侧栏各是一个面板，不用 `set_dock` 换；当前标签只经 `Workspace::set_active_tab` 改。

### SSH

- 所有登录都走核心的 `SshConnector`；known_hosts 写在数据目录，不碰 `~/.ssh/known_hosts`。没人能回答的问答（测试连接、exec、重连）当场拒绝，不能挂住。
- 算法按 OpenSSH 默认（另加了老堡垒机要的 ECDH nistp 和 ≥ 2048 位的 DH）。
- 只有网络类错误才自动重连。
- 外部连接只开一个通道：用户的堡垒机多开通道就断会话。
- 右侧栏工具的命令写成一行 `sh -c '…'`，要经得过 bash、zsh、fish、csh；各工具都有用本机 `sh` 真跑脚本的测试。

### 文件传输

- 每一步是一行 `sh -c '…' sh 参数`，脚本里不能有单引号和 `!`，路径只作参数传；测试用本机 `sh` 真跑（`crates/shellrs-ssh/src/transfer.rs`），`src/ssh/transport.rs` 里另有一条经进程内 SSH 服务器的端到端测试。
- 命令经 `TerminalTransportCommand::StartCommand` 在终端的连接上启动，启动后交给传输的工作线程自己读写；主机不认识的回答一律当失败，主机给的名字不能带路径。

### 端口转发

- 规则在 store，运行状态在 `ForwardManager`；首次登录可以问，重连不问；发现断线靠处理器被丢弃。

### 升级、发布、平台

- **Windows** 只在 CI 上编译和测试：`windows.yml` 按路径触发，新加 `cfg(windows)` 代码的文件要加进那个列表。发布版是 GUI 子系统。C 运行库静态链接（`.cargo/config.toml` 的 `+crt-static`）：动态链接要 `VCRUNTIME140.dll`，LTSC 版和没装 VC++ 运行库的系统启动不了；`scripts/windows-runtime.ps1` 在 CI 里查 exe 的导入表。
- 在线升级在 UI 测试里没有真实服务，测试从不联网。
- **发布走 tag**（`docs/release.md`）：写好两份更新日志后运行 `scripts/release.sh x.y.z`，它改版本号、提交、打 tag、推送；流水线给包签名（minisign，私钥在 `release` 环境的 `MINISIGN_SECRET_KEY`）、建 GitHub Release，最后把清单提交到 `update` 分支的 `v1/<通道>.json`，客户端从 raw.githubusercontent.com 读它；不用 R2、Cloudflare 和官网。没有 Apple 的 secrets 时 macOS 包是 ad-hoc 签名、不公证。更新日志由改版本号的提交写，中英两份（`CHANGELOG.zh.md`、`CHANGELOG.md`）都要有，功能提交不改。
- macOS 上关窗口是隐藏，⌘Q 才退出。

### 同步核心（`crates/shellrs-sync`）

- 时间和随机数只经 `Env` 取（守卫测试查），一轮的结果只取决于参数；测试用 `FixedEnv`。
- 一轮分两步：`pull` 什么都不写；应用在一个事务里应用改动并保存它返回的状态，期间记录有变就丢掉结果重来；之后才 `push`。
- 应用交出的每条记录带 `edited_ms`：只在用户改（或替用户改，如删凭据后主机改回自己登录）时变；同步写入的记录保持原值，新建的为 0。
- 秘密（`secret.*` 记录：登录密码、代理密码、凭据的密码、口令、私钥）一律同步，没有开关；明文只在加密的设备文件里：`SyncState` 里只有用数据密钥加密的密文（`to_bytes` 里找不到明文，有测试查），存进数据库的上传摘要也不哈希秘密内容；交给应用的 `Secret` 的 `Debug` 不显示内容、用完清零。
- 应用这一轮读不出秘密（存秘密的地方读不了）要把 `secrets_read` 设成 false，读不出的单条（私钥文件）和一轮带来、还没写进去的秘密列进 `unreadable`：它们原样保留，绝不当成删除，`pull` 再交一次。没写进去的秘密按同步键记在数据库的 `sync_pending`（只有键），和改动同一个事务写下，写成功才删。
- 同步的密钥（`Keyring::to_bytes`，应用存在秘密账户 `sync:keyring` 里）只带最新 8 代数据密钥；更旧的在文件或状态用到时，用包裹密钥从 `vault.json` 取回，不用密码。`vault.json` 保留全部代。
- 只读变过的秘密：编辑时间（`secret_edits`）等于 `SyncState::edit_seen` 的不读，列进 `unchanged`，由状态里存的那份顶上；用户删除秘密也要记编辑时间，否则删除传不出去。
- 格式（`FORMAT.md`）发布后不能改；`testdata/v1` 的金样文件以后每个版本都要能读。发布前不留兼容旧数据的代码：每个字段都写出、读时缺了就算坏记录，不补默认值。发布后新加的字段（`FORMAT.md` 里标 ¹，如片段分类的 `parent`、`sort_order`）读时缺了按旧版本的样子补（`Reader::later`），旧版本靠「不认识的字段原样保留」带着它走；本机默认值要和补的一样，否则升级后每条都被当成过时的副本。
- WebDAV 后端（`store/webdav.rs`）只用 PROPFIND、GET、PUT、DELETE、MKCOL。ETag 不论强弱都当标签（Apache 给刚写的文件头一秒弱 ETag，`If-Match` 永远不匹配弱的），条件写被拒先 PROPFIND 核对再算冲突；服务器不给 ETag 就用修改时间加大小拼，离服务器的 `Date` 太近就带疑点标记；列目录不给、读文件却给 ETag 的（nginx），读写也改用时间加大小，否则每轮都把别的设备的文件重下一遍。测试起的 Apache 要在自己的进程组里（`process_group(0)`），它退出时会给整个进程组发 TERM，把 cargo 和 shell 一起杀掉。
- S3 后端（`store/s3.rs`）自己签名（SigV4，`hmac` + `sha2`，不引 SDK），有对照 AWS 官方示例的测试；它和本地文件夹后端是仅有的两处可以读系统时间的地方（签名要用，守卫测试里列着）。条件写（`If-None-Match: *`、`If-Match`）被服务回「未实现」就改成先 HEAD 再写；时钟偏差过大被拒时按服务的 `Date` 校正重签一次。
- 这个 crate 是 MIT：只放作者自己写的代码，不从别人贡献的 GPL 代码里复制。

### SSH 核心（`crates/shellrs-ssh`）

- **核心不产出文案。** 失败是 `anyhow` 链里的类型化叶子错误，链要留着：是不是网络错误、缺不缺凭据都沿链判断。桌面端在 `ssh/describe.rs`、`transfer/describe.rs` 沿链逐节翻译、用「: 」连起来。核心类型不加 `#[non_exhaustive]`，加了变体 `describe` 编译就报错。
- **核心不依赖应用。** 秘密经 `SecretSource` 读（阻塞，放进 `spawn_blocking`）；agent、默认私钥目录、NAT64 是 connector 的配置。核心的 Rust API 在 workspace 里随时可改。

### 主机核心（`crates/shellrs-hosts`）

- **核心不产出文案。** label 和错误都是类型（`ProtocolKind`、`HostDraftError`、`Failure`、`SecretError`、`JobError`、`LoginError` 等），桌面在 `host/describe.rs`、`sync/describe.rs` 用扩展 trait 翻译（`KindLabel::label()`、`ErrorMessage::message()`，调用处 `use … as _`）。要起名字的地方（复制主机的「副本」、新本地终端的默认名）由调用方把名字传进来。
- **`HostData` 不通知、不碰线程。** 写方法改内存、写库，失败、转发设置的变化和 `Changed` 放进 `take_events()`，秘密的写入放进 `take_secret_jobs()`（`SecretJob` 跨线程跑，结果交回 `secret_done`）。外壳上同名带 `cx` 的写方法优先被找到，所以绕不过 `settle`；新加写方法要加进 `host/store.rs` 的 `settled!` 列表。经 `DerefMut` 只调读方法和 `*_unnotified`。
- **一轮同步经 `RoundHost`**：`run_round` 只经 `with_data`（短暂拿数据）和 `background`（阻塞的步骤）和外面打交道；桌面用 `WeakEntity<HostStore>` 加 `AsyncApp` 实现，每次用完数据都 `settle`。什么时候同步留在应用的 `SyncService`。`background` 收装箱的闭包（`Blocking<R>`）、打开的存储包成 `Remote`：不然 rustc 证不出这一轮的 future 是 `Send`，放不上多线程运行时。
- **核心不依赖应用。** `Host.sidebar_tool` 存 `ToolKind::key` 的字符串（不认识的原样留着）；品牌色和按地址取的颜色是 `u32`，应用用 `rgb()` 转；User-Agent 和设备信息由应用交给 `SyncServices::system`。
- 测试夹具：`HostDatabase::in_memory` 在 `testing` feature，应用只在 dev-dependency 里打开；`HostData::seed()` 留在普通构建里，因为 `tests/ui/` 链接的是正常编出来的库。

### 终端

- ZMODEM 先于 alacritty 拿到输出；传输期间键只认 Ctrl+C。
- 查找条是终端 `div` 的兄弟，不是子元素，否则输入会发给 shell。
- 程序的查询（光标位置、颜色等）在解析线程上按顺序回答，别交回 UI 线程。
- 同步更新（DEC 2026）超时要解析线程自己收尾，否则屏幕会冻住。
- 终端字体、颜色、高亮规则走全局量（终端模块不依赖 settings）。

## 测试约定

- **一个测试程序**：UI 测试都在 `tests/ui/`（`main.rs` 按功能分模块，共用的在 `support/`），不在 `tests/` 下另开文件（每个都要再链接一遍 GPUI）。
- **全部注入**：`Workspace::new_with_services` 注入假实现；测试不碰用户的服务器、数据目录、设置文件、真实套接字和网络。`HostStore::seed()` 是夹具。
- **保险库**：测试里没装保险库（`vault::install`），经 `shellrs_ssh::files` 读写的文件都是明文；要测加密和应用锁就在临时文件夹里建 `Vault::with_cost`，用很小的 Argon2id 参数。
- 被查询的元素带稳定的 id，自定义 `div` 还要加 `.test_support()`。
- 不用右键菜单驱动测试，直接派发菜单会派发的动作；测试模块显式导入类型（`use gpui_kit::*` 会遮蔽 `#[test]`）。
- 动作和 `set_active` 是延迟执行的：先 `run_until_parked` 再 `render_frame`；工作线程的事件要推进时钟或 `cx.wait_for`。
- 看不见的换个法子查：密码查 `InMemorySecretStore`，按钮禁用看能否取得焦点。
- 单元测试起子进程一律经 `crate::testing`。
