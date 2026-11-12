//! ShellRS: an SSH host manager with real SSH and local terminals.
//!
//! Modules are organized by capability so they can become crates later:
//! `host` (主机管理), `terminal` (终端), `ssh` (SSH 连接), `transfer`
//! (文件传输), `forward` (端口转发), `credential`
//! (凭据), `monitor` (系统监控), `services` (系统服务), `processes` (进程管理),
//! `netstat` (网络连接), `history` (历史命令), `snippets` (命令片段),
//! `secrets` (秘密的读写接口), `vault` (加密保险库), `app_lock` (应用锁), `settings` (设置), `instance` (单实例与外部连接的链接), `update` (在线升级),
//! `connection` 与 `shared` (跨模块共用), `i18n` (多语言), `workspace` (窗口壳), `app` (动作、快捷键、资源).

pub mod app;
pub mod app_lock;
pub mod connection;
pub mod credential;
pub mod forward;
pub mod history;
pub mod host;
pub mod i18n;
pub mod instance;
pub mod monitor;
pub mod netstat;
pub mod processes;
pub mod project;
pub mod secrets;
pub mod serial;
pub mod services;
pub mod settings;
pub mod shared;
pub mod snippets;
pub mod ssh;
pub mod sync;
pub mod telnet;
pub mod terminal;
pub mod tmux;
pub mod transfer;
pub mod update;
pub mod vault;
pub mod workspace;

#[cfg(test)]
mod testing;

// The text of every language, from `locales/`: see `i18n`.
rust_i18n::i18n!("locales", fallback = "zh-CN");

/// Initialize GPUI Kit, locale, key bindings and global actions. Call once
/// inside `Application::run` before creating any view.
pub fn init(cx: &mut gpui_kit::App) {
    // The identity of the taskbar button, set before any window opens: on
    // Windows the AppUserModelID.
    cx.set_app_identity(app::app_identity(), "ShellRS");
    app::init(cx);
}
