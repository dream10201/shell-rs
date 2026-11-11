// A release build on Windows is a GUI program, so opening it does not bring
// up a console window as well. Debug builds keep theirs for the logs.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

use gpui_kit::component::{Root, notification::Notification};
use gpui_kit::*;

use shellrs::host::{HostDatabase, HostStore};
use shellrs::settings::{SettingsProblem, SettingsStore};

fn main() {
    keep_memory_private();
    let args = shellrs::instance::command_line_arguments();
    // `ShellRS ssh://user@host:port` or `ShellRS -url … -newtab 名称`: a
    // bastion host opening ShellRS the way it opens Xshell. On every build,
    // a Windows release build above all, where bastion hosts run it.
    let link = shellrs::instance::link_arguments(&args);
    // One ShellRS per data directory. Each keeps the hosts in memory and
    // writes its changes through, so a second one would write over the
    // first one's; opened again, ShellRS brings the running one forward
    // and hands it the link.
    if shellrs::instance::activate_running_app(&shellrs::app::instance_socket_path(), link.clone())
    {
        return;
    }
    let app = gpui_kit::application().with_assets(shellrs::app::AppAssets);
    // macOS: the Dock icon of a ShellRS whose window was closed, which only
    // hid it (see `hide_when_closed`).
    app.on_reopen(|cx| {
        if let Some(shellrs::app::MainWindow(window)) = cx.try_global().copied() {
            window
                .update(cx, |_, window, cx| shellrs::app::bring_forward(window, cx))
                .ok();
        }
    });
    app.run(|cx| {
        // Everything ShellRS keeps is sealed in the vault, the settings
        // too: until the password is given the window is in the system's
        // language and the default theme.
        let vault = std::sync::Arc::new(shellrs::vault::Vault::new(shellrs::app::vault_dir()));
        shellrs::vault::install(vault.clone());
        shellrs::i18n::set_locale(shellrs::settings::InterfaceLanguage::default().resolved());
        shellrs::init(cx);
        // 显示或隐藏 ShellRS takes its key from the system; tests never do.
        shellrs::app::listen_for_system_hotkey(cx);
        // ShellRS's menu and 窗口, whose items have ⌘H and ⌘M.
        #[cfg(target_os = "macos")]
        shellrs::app::show_menu_bar(cx);
        shellrs::app::show_logo_when_unbundled(cx);
        cx.activate(true);

        let saved = shellrs::workspace::WindowState::load(&shellrs::app::window_state_path());
        let remember = saved
            .as_ref()
            .map(shellrs::workspace::WindowState::remember)
            .unwrap_or_default();
        let options = shellrs::workspace::window_options(saved.as_ref(), remember, cx);
        cx.spawn(async move |cx| {
            let window = cx
                .open_window(options, |window, cx| {
                    window.activate_window();
                    window.set_window_title("ShellRS");
                    #[cfg(windows)]
                    shellrs::app::listen_for_tray(window, cx);
                    shellrs::app::hide_when_closed(window, cx);
                    // Drawn by the processor (a virtual machine without 3D),
                    // ShellRS draws fewer frames.
                    shellrs::app::note_renderer(window, cx);
                    let lock = cx.new(|_| shellrs::app_lock::AppLock::new(vault.clone()));
                    shellrs::app_lock::AppLock::install(lock.clone(), cx);
                    lock.update(cx, |lock, _| {
                        lock.watch_screen(shellrs::app_lock::system_screen())
                    });
                    shellrs::settings::apply(shellrs::settings::AppSettings::default(), window, cx);
                    let server = match shellrs::instance::InstanceServer::reserve(
                        shellrs::app::instance_socket_path(),
                    ) {
                        Ok(server) => server,
                        Err(error) => {
                            let failure =
                                cx.new(|_| shellrs::workspace::StartupFailure::new(&error));
                            return cx.new(|cx| Root::new(failure, window, cx));
                        }
                    };
                    let shell = cx.new(|cx| {
                        shellrs::workspace::AppShell::new(
                            lock,
                            Some(server),
                            move |server, pending_links, window, cx| {
                                let (settings, settings_problem) = open_settings();
                                shellrs::settings::apply(settings.settings(), window, cx);
                                let (store, problem) = open_store(&vault);
                                let store = cx.new(|cx| {
                                    let mut store = store;
                                    store.keep_at(shellrs::app::database_path(), cx);
                                    store
                                });
                                let settings = cx.new(|_| settings);
                                let workspace = cx.new(|cx| {
                                    shellrs::workspace::Workspace::new_with_instance_server(
                                        store, settings, server, window, cx,
                                    )
                                });
                                let settings_problem =
                                    settings_problem.map(|problem| problem.message());
                                for problem in [problem, settings_problem].into_iter().flatten() {
                                    shellrs::workspace::notify_once_open(
                                        Notification::error(problem),
                                        window,
                                        cx,
                                    );
                                }
                                for link in link.into_iter().chain(pending_links) {
                                    shellrs::workspace::open_link_once_open(
                                        &workspace, link, window, cx,
                                    );
                                }
                                workspace.into()
                            },
                            window,
                            cx,
                        )
                    });
                    cx.new(|cx| Root::new(shell, window, cx))
                })
                .expect("failed to open window");
            cx.update(|cx| cx.set_global(shellrs::app::MainWindow(window.into())));
        })
        .detach();
    });
    // On Windows the event loop ends and `run` returns: an update that is
    // to be installed now starts its setup, which waits for this process.
    shellrs::update::start_handed_over();
}

/// Load the hosts kept in the vault. If the database cannot be opened the
/// window still comes up, on a store that forgets everything when it closes;
/// the returned message says so.
///
/// Passwords go into the vault's sealed file, pasted and generated private
/// keys beside the database; the credentials follow them when the data
/// folder has moved.
fn open_store(vault: &std::sync::Arc<shellrs::vault::Vault>) -> (HostStore, Option<SharedString>) {
    let secrets = std::sync::Arc::new(shellrs::vault::VaultSecretStore::new(vault.clone()));
    match load_store() {
        // Keys are kept only with a database to remember them: a store
        // that forgets everything would leave them behind unused.
        Ok(store) => {
            let mut store = store
                .with_secrets(secrets)
                .with_key_dir(shellrs::app::keys_dir())
                .with_terminal_output_dir(shellrs::app::terminal_output_dir());
            let problem = store.adopt_moved_keys();
            (store, problem)
        }
        Err(error) => {
            eprintln!("shellrs: cannot open the local database: {error}");
            (
                HostStore::empty().with_secrets(secrets),
                Some(shellrs::app::database_unavailable(&error)),
            )
        }
    }
}

/// Load the settings: in the language they choose, since the defaults that
/// become the user's own (the example highlight rules of a first start)
/// are made while reading.
fn open_settings() -> (SettingsStore, Option<SettingsProblem>) {
    let (settings, problem) = SettingsStore::load(shellrs::app::settings_path());
    let locale = settings.settings().language.resolved();
    if locale == shellrs::i18n::locale() {
        return (settings, problem);
    }
    shellrs::i18n::set_locale(locale);
    SettingsStore::load(shellrs::app::settings_path())
}

/// The database, which runs in memory: as its sealed file left it, or new.
fn load_store() -> anyhow::Result<HostStore> {
    let path = shellrs::app::database_path();
    let image = match shellrs_ssh::files::read(&path) {
        Ok(image) => Some(image),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    // Before a newer schema replaces it, so the version that wrote it can
    // still open its data.
    if let Some(version) = image.as_deref().and_then(HostDatabase::outdated_image) {
        let mut backup = path.clone().into_os_string();
        backup.push(format!(".v{version}.bak"));
        if let Err(error) = std::fs::copy(&path, &backup) {
            eprintln!("shellrs: cannot back up the database before migrating it: {error}");
        }
    }
    Ok(HostStore::load(HostDatabase::from_image(
        image.as_deref(),
    )?)?)
}

/// Keep the keys in this process's memory out of core dumps and out of
/// reach of other processes of the same user. Linux only; a program
/// started from here (a local terminal's shell) is not affected.
fn keep_memory_private() {
    #[cfg(target_os = "linux")]
    // SAFETY: a plain system call on this process.
    unsafe {
        libc::prctl(libc::PR_SET_DUMPABLE, 0);
    }
}
