//! The host data as a GPUI entity: what the workspace creates once and
//! every panel and dialog observes. [`HostData`] holds the data and writes
//! it through to the database; this says what each change did, as
//! notifications and [`HostStoreEvent`]s, and runs the keychain writes on
//! the background executor.

use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui_kit::{AppContext as _, Context, EventEmitter, SharedString, Task};
use zeroize::Zeroizing;

use crate::i18n::t;
use crate::secrets::{SecretRef, SharedSecretStore};

use super::{
    ApplyError, CredentialDraft, CredentialId, ErrorMessage as _, ForwardDraft, ForwardId,
    GroupDraft, GroupId, GroupPlan, HostData, HostDatabase, HostDraft, HostEvent, HostId, HostNode,
    HostOs, NodeDrop, SavedTerminalId, SavedTerminalTab, SecretJob, SecretWork, SecretsWritten,
    SnippetCategoryDraft, SnippetCategoryId, SnippetDraft, SnippetDrop, SnippetId, SnippetNode,
    SshLink, SyncExport, TerminalDrop, TerminalGroupId, TerminalNode,
};

/// The single source of truth for hosts and everything kept with them (see
/// [`HostData`]), shared with every panel and dialog; consumers observe it.
///
/// Its mutators take a `Context`: they change the data, which writes the
/// change through to the database, then notify observers and pass on what
/// could not be kept as `PersistFailed`. The data's `*_unnotified` variants
/// and everything that only reads are reached through `Deref`.
pub struct HostStore {
    data: HostData,
    /// Where the database, which runs in memory, is kept as a file.
    file: Option<DatabaseFile>,
    /// The app is locked: the data is to be let go of as soon as the file
    /// has all of it.
    locked: bool,
    /// The data is in the file alone, until [`Self::reload`].
    unloaded: bool,
    /// The keychain writes running: they come back to the data.
    secret_jobs: usize,
    /// What open terminals said while the data was unloaded.
    held: Vec<Held>,
}

/// A change kept until the data is back to take it.
enum Held {
    Directory(SavedTerminalId, String),
    Title(SavedTerminalId, SharedString),
    Tabs(Vec<SavedTerminalTab>),
}

/// The database's file and how far it is behind.
struct DatabaseFile {
    path: PathBuf,
    /// The database's change count the file has, or is being written with.
    saved: u64,
    /// The newest image written, so that a slow older write cannot land
    /// over it.
    written: Arc<Mutex<u64>>,
    saving: Option<Task<()>>,
    _quit: gpui_kit::Subscription,
}

/// How long a change waits for more before the file is written.
const SAVE_DELAY: Duration = Duration::from_millis(300);

/// What the store tells the workspace about, beyond plain change notification.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HostStoreEvent {
    /// A change stands in memory but could not be kept: the database or
    /// the keychain refused it. Already worded for the user.
    PersistFailed(SharedString),
    /// A port forward's rule changed in a way its running forward has to
    /// restart for.
    ForwardSettingsChanged(ForwardId),
}

impl EventEmitter<HostStoreEvent> for HostStore {}

impl Deref for HostStore {
    type Target = HostData;

    fn deref(&self) -> &HostData {
        &self.data
    }
}

impl DerefMut for HostStore {
    fn deref_mut(&mut self) -> &mut HostData {
        &mut self.data
    }
}

/// The data's mutators, each with the `cx` to settle what it did.
macro_rules! settled {
    ($(fn $name:ident($($arg:ident: $ty:ty),*) $(-> $ret:ty)?;)*) => {
        $(
            pub fn $name(&mut self, $($arg: $ty,)* cx: &mut Context<Self>) $(-> $ret)? {
                let result = self.data.$name($($arg),*);
                self.settle(cx);
                result
            }
        )*
    };
}

impl HostStore {
    pub fn new(data: HostData) -> Self {
        Self {
            data,
            file: None,
            locked: false,
            unloaded: false,
            secret_jobs: 0,
            held: Vec::new(),
        }
    }

    /// Keep the database, which runs in memory, in the file at `path`:
    /// written shortly after each change, when the app locks and when it
    /// quits.
    pub fn keep_at(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.file = Some(DatabaseFile {
            path,
            // What opening it changed (a migration, a key that moved) is
            // still to be written.
            saved: 0,
            written: Arc::default(),
            saving: None,
            _quit: cx.on_app_quit(|this, cx| {
                this.save_now(cx);
                async {}
            }),
        });
    }

    /// Write the database's file now, if it is behind. A vault that is
    /// locked cannot take it: it is written once unlocked.
    pub fn save_now(&mut self, cx: &mut Context<Self>) {
        if self.unloaded {
            return;
        }
        let Some(file) = &mut self.file else {
            return;
        };
        file.saving = None;
        let Some(image) = self.data.database_image(file.saved) else {
            return;
        };
        let written = image
            .map_err(std::io::Error::other)
            .and_then(|(image, changes)| {
                write_image(&file.path, &image, changes, &file.written).map(|()| changes)
            });
        match written {
            Ok(changes) => file.saved = changes,
            Err(error) if crate::vault::is_locked(&error) => {}
            Err(error) => cx.emit(HostStoreEvent::PersistFailed(t!(
                "host.store.save_failed",
                error = error
            ))),
        }
    }

    /// The app locked: write the file and keep nothing of it in memory.
    /// Hosts that are not saved stay, as the tabs open on them do. A store
    /// without a file, or one the file could not take, stays as it is.
    pub fn unload(&mut self, cx: &mut Context<Self>) {
        self.locked = true;
        self.save_now(cx);
        self.unload_once_kept(cx);
    }

    fn unload_once_kept(&mut self, cx: &mut Context<Self>) {
        let kept = self
            .file
            .as_ref()
            .is_some_and(|file| self.data.database_changes() == Some(file.saved));
        if !self.locked || self.unloaded || self.secret_jobs > 0 || !kept {
            return;
        }
        self.settle(cx);
        if let Some(file) = &mut self.file {
            file.saving = None;
        }
        self.data.unload();
        self.unloaded = true;
        cx.notify();
    }

    /// Whether the data is in the file alone: what reads the store then
    /// finds nothing, which does not mean it was deleted.
    pub fn is_unloaded(&self) -> bool {
        self.unloaded
    }

    /// The app unlocked: read the data back from the file, or write what
    /// changed while locked if it never left.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.locked = false;
        if !self.unloaded {
            self.save_now(cx);
            return;
        }
        let Some(file) = &mut self.file else {
            return;
        };
        let loaded = shellrs_ssh::files::read(&file.path)
            .map_err(anyhow::Error::from)
            .and_then(|image| Ok(HostDatabase::from_image(Some(&image))?))
            .and_then(|database| Ok(self.data.reload(database)?));
        if let Err(error) = loaded {
            cx.emit(HostStoreEvent::PersistFailed(
                crate::app::database_unavailable(&error),
            ));
            return;
        }
        // The file is what was just read; the new database counts its
        // changes from the start.
        file.saved = self.data.database_changes().unwrap_or(0);
        file.written = Arc::default();
        self.unloaded = false;
        for held in std::mem::take(&mut self.held) {
            match held {
                Held::Directory(id, directory) => {
                    self.data.set_saved_terminal_directory(id, directory);
                }
                Held::Title(id, title) => {
                    self.data.set_saved_terminal_title(id, title);
                }
                Held::Tabs(tabs) => self.data.set_saved_terminal_tabs(tabs),
            }
        }
        self.settle(cx);
        cx.notify();
    }

    /// Write the file once the changes have stopped coming.
    fn save_soon(&mut self, cx: &mut Context<Self>) {
        if self.unloaded {
            return;
        }
        let changes = self.data.database_changes();
        let Some(file) = &mut self.file else {
            return;
        };
        if file.saving.is_some() || changes == Some(file.saved) {
            return;
        }
        file.saving = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_DELAY).await;
            let Ok(Some((path, image, changes, written))) = this.update(cx, |this, _| {
                let file = this.file.as_mut()?;
                let image = this.data.database_image(file.saved).and_then(Result::ok);
                let Some((image, changes)) = image else {
                    // Nothing to write, or a batch is open: the change
                    // that ends it asks again.
                    if let Some(task) = file.saving.take() {
                        task.detach();
                    }
                    return None;
                };
                Some((file.path.clone(), image, changes, file.written.clone()))
            }) else {
                return;
            };
            let result = cx
                .background_spawn(async move { write_image(&path, &image, changes, &written) })
                .await;
            let _ = this.update(cx, |this, cx| {
                let Some(file) = &mut this.file else {
                    return;
                };
                file.saving = None;
                match result {
                    Ok(()) => {
                        file.saved = file.saved.max(changes);
                        // What changed while this one was written.
                        this.save_soon(cx);
                    }
                    // Written once unlocked, or with the next change: not
                    // tried again and again meanwhile.
                    Err(error) if crate::vault::is_locked(&error) => {}
                    Err(error) => cx.emit(HostStoreEvent::PersistFailed(t!(
                        "host.store.save_failed",
                        error = error
                    ))),
                }
            });
        }));
    }

    /// An empty memory-only store.
    pub fn empty() -> Self {
        Self::new(HostData::empty())
    }

    /// Read everything back from `database` and keep writing to it.
    pub fn load(database: HostDatabase) -> rusqlite::Result<Self> {
        HostData::load(database).map(Self::new)
    }

    /// Sample data for tests; see [`HostData::seed`].
    pub fn seed() -> Self {
        Self::new(HostData::seed())
    }

    /// Attach where passwords are kept. `main` does this once at startup; the UI
    /// tests attach an in-memory store instead.
    pub fn with_secrets(self, secrets: SharedSecretStore) -> Self {
        Self::new(self.data.with_secrets(secrets))
    }

    /// Keep pasted and generated private keys in `dir`.
    pub fn with_key_dir(self, dir: impl Into<PathBuf>) -> Self {
        Self::new(self.data.with_key_dir(dir))
    }

    /// Keep the saved terminals' output in `dir`.
    pub fn with_terminal_output_dir(self, dir: impl Into<PathBuf>) -> Self {
        Self::new(self.data.with_terminal_output_dir(dir))
    }

    /// See [`HostData::adopt_moved_keys`]; says what could not be saved.
    pub fn adopt_moved_keys(&mut self) -> Option<SharedString> {
        self.data
            .adopt_moved_keys()
            .map(|failure| failure.message())
    }

    /// Pass on what the data did: what could not be kept as
    /// `PersistFailed`, a forward to restart, and one notification for
    /// whatever changed. The keychain writes it queued go to the background.
    pub fn settle(&mut self, cx: &mut Context<Self>) {
        for job in self.data.take_secret_jobs() {
            self.run_secret_job(job, cx).detach();
        }
        let mut changed = false;
        for event in self.data.take_events() {
            match event {
                HostEvent::Failed(failure) => {
                    cx.emit(HostStoreEvent::PersistFailed(failure.message()));
                }
                HostEvent::ForwardSettingsChanged(id) => {
                    cx.emit(HostStoreEvent::ForwardSettingsChanged(id));
                }
                HostEvent::Changed => changed = true,
            }
        }
        if changed {
            cx.notify();
        }
        self.save_soon(cx);
    }

    settled! {
        fn insert_temporary(draft: HostDraft, password: Option<Zeroizing<String>>) -> HostId;
        fn remove_temporary(id: HostId);
        fn place_group(plan: GroupPlan) -> Option<GroupId>;
        fn insert(draft: HostDraft) -> HostId;
        fn update(id: HostId, draft: HostDraft) -> bool;
        fn remove(id: HostId) -> bool;
        fn insert_group(draft: GroupDraft) -> GroupId;
        fn update_group(id: GroupId, draft: GroupDraft) -> bool;
        fn insert_forward(draft: ForwardDraft) -> Option<ForwardId>;
        fn update_forward(id: ForwardId, draft: ForwardDraft) -> bool;
        fn remove_forward(id: ForwardId) -> bool;
        fn insert_credential(draft: CredentialDraft) -> CredentialId;
        fn update_credential(id: CredentialId, draft: CredentialDraft) -> bool;
        fn remove_credential(id: CredentialId) -> Vec<HostId>;
        fn set_group_expanded(id: GroupId, expanded: bool) -> bool;
        fn set_all_groups_expanded(expanded: bool) -> bool;
        fn move_node(source: HostNode, drop: NodeDrop) -> bool;
        fn remove_group(id: GroupId) -> Vec<HostId>;
        fn record_connection(id: HostId);
        fn set_host_os(id: HostId, os: Option<HostOs>);
        fn set_host_sidebar_tool(id: HostId, tool: Option<SharedString>);
        fn save_secret(secret: SecretRef, value: Option<String>);
        fn insert_terminal_group(name: SharedString) -> TerminalGroupId;
        fn rename_terminal_group(id: TerminalGroupId, name: SharedString) -> bool;
        fn set_terminal_group_expanded(id: TerminalGroupId, expanded: bool) -> bool;
        fn remove_terminal_group(id: TerminalGroupId) -> Vec<SavedTerminalId>;
        fn remove_saved_terminal(id: SavedTerminalId) -> bool;
        fn move_terminal_node(source: TerminalNode, drop: TerminalDrop) -> bool;
        fn insert_snippet(draft: SnippetDraft) -> SnippetId;
        fn update_snippet(id: SnippetId, draft: SnippetDraft) -> bool;
        fn remove_snippet(id: SnippetId) -> bool;
        fn insert_snippet_category(draft: SnippetCategoryDraft) -> SnippetCategoryId;
        fn update_snippet_category(id: SnippetCategoryId, draft: SnippetCategoryDraft) -> bool;
        fn remove_snippet_category(id: SnippetCategoryId) -> bool;
        fn move_snippet_node(source: SnippetNode, drop: SnippetDrop) -> bool;
        fn apply_sync(
            changes: &[shellrs_sync::Change],
            state: &[u8],
            export: &SyncExport
        ) -> Result<SecretWork, ApplyError>;
        fn adopt_synced_secrets(written: &SecretsWritten);
    }

    /// A saved terminal's shell is in `directory` now. Kept for later
    /// while the data is unloaded, as the next two are: open terminals go
    /// on under the lock.
    pub fn set_saved_terminal_directory(
        &mut self,
        id: SavedTerminalId,
        directory: String,
        cx: &mut Context<Self>,
    ) {
        if self.unloaded {
            self.held.push(Held::Directory(id, directory));
            return;
        }
        self.data.set_saved_terminal_directory(id, directory);
        self.settle(cx);
    }

    pub fn set_saved_terminal_title(
        &mut self,
        id: SavedTerminalId,
        title: SharedString,
        cx: &mut Context<Self>,
    ) {
        if self.unloaded {
            self.held.push(Held::Title(id, title));
            return;
        }
        self.data.set_saved_terminal_title(id, title);
        self.settle(cx);
    }

    pub fn set_saved_terminal_tabs(&mut self, tabs: Vec<SavedTerminalTab>, cx: &mut Context<Self>) {
        if self.unloaded {
            self.held.push(Held::Tabs(tabs));
            return;
        }
        self.data.set_saved_terminal_tabs(tabs);
        self.settle(cx);
    }

    /// Connect to what an `ssh://` link from outside names
    /// without saving it (外部连接); see [`HostData::insert_external`].
    pub fn insert_external(&mut self, link: SshLink, cx: &mut Context<Self>) -> HostId {
        let id = self.data.insert_external(link.draft(), link.password);
        self.settle(cx);
        id
    }

    pub fn insert_external_unnotified(&mut self, link: SshLink) -> HostId {
        self.data
            .insert_external_unnotified(link.draft(), link.password)
    }

    /// Copy a host as `<name> 副本` (`<name> copy`), placed right after the
    /// original. The name is the copy's own from then on, in the language it
    /// was made in.
    pub fn duplicate(&mut self, id: HostId, cx: &mut Context<Self>) -> Option<HostId> {
        let name = t!("host.store.copy_name", name = self.host(id)?.name);
        let copy = self.data.duplicate(id, name);
        self.settle(cx);
        copy
    }

    /// Keep a new terminal in `group`, or at the top, named after the group
    /// (`shellrs 2`, or `终端 2` at the top) and starting in `directory`.
    /// `None` when the group is gone.
    pub fn insert_saved_terminal(
        &mut self,
        group: Option<TerminalGroupId>,
        directory: Option<String>,
        cx: &mut Context<Self>,
    ) -> Option<SavedTerminalId> {
        let id = self
            .data
            .insert_saved_terminal(group, directory, &t!("host.saved_terminal.name"));
        self.settle(cx);
        id
    }

    pub fn insert_saved_terminal_unnotified(
        &mut self,
        group: Option<TerminalGroupId>,
        directory: Option<String>,
    ) -> Option<SavedTerminalId> {
        self.data.insert_saved_terminal_unnotified(
            group,
            directory,
            &t!("host.saved_terminal.name"),
        )
    }

    /// Name a terminal by hand, or with `None` give it a name of its group's
    /// again (`shellrs 3`), which gives way to the titles of its tab.
    pub fn rename_saved_terminal(
        &mut self,
        id: SavedTerminalId,
        name: Option<SharedString>,
        cx: &mut Context<Self>,
    ) -> bool {
        let renamed = self
            .data
            .rename_saved_terminal(id, name, &t!("host.saved_terminal.name"));
        self.settle(cx);
        renamed
    }

    pub fn rename_saved_terminal_unnotified(
        &mut self,
        id: SavedTerminalId,
        name: Option<SharedString>,
    ) -> bool {
        self.data
            .rename_saved_terminal_unnotified(id, name, &t!("host.saved_terminal.name"))
    }

    /// [`HostData::save_secret`], for a caller that has to know how it went:
    /// the failure, already reported as `PersistFailed`.
    pub fn write_secret(
        &mut self,
        secret: SecretRef,
        value: Option<String>,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), SharedString>> {
        let job = self.data.secret_job(secret, value);
        self.run_secret_job(job, cx)
    }

    /// Run a keychain write on the background executor, and settle how it
    /// went.
    fn run_secret_job(
        &mut self,
        job: SecretJob,
        cx: &mut Context<Self>,
    ) -> Task<Result<(), SharedString>> {
        self.secret_jobs += 1;
        cx.spawn(async move |this, cx| {
            let outcome = cx
                .background_executor()
                .spawn(async move { job.run() })
                .await;
            let failed = outcome.failure().map(|failure| failure.message());
            this.update(cx, |store, cx| {
                store.secret_jobs -= 1;
                store.data.secret_done(outcome);
                store.settle(cx);
                // Locked meanwhile: nothing is waited for any more.
                store.save_now(cx);
                store.unload_once_kept(cx);
            })
            .ok();
            failed.map_or(Ok(()), Err)
        })
    }

    /// Run `change` with every database write it makes in one transaction
    /// (see [`HostData::begin_batch`]). Nested calls join the outer one.
    pub fn in_one_transaction<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Self, &mut Context<Self>) -> R,
    ) -> R {
        if !self.data.begin_batch() {
            return change(self, cx);
        }
        let result = change(self, cx);
        self.data.end_batch();
        self.settle(cx);
        result
    }

    /// Run `change` and say what of it could not be written to the
    /// database (or a key file deleted), for a caller that says so itself
    /// rather than in a notification. The failures are
    /// reported as `PersistFailed` all the same, and the change stands in
    /// memory as it always does. Keychain writes are not among them: they
    /// finish later, and [`Self::write_secret`] says how.
    pub fn capture_failures<R>(
        &mut self,
        cx: &mut Context<Self>,
        change: impl FnOnce(&mut Self, &mut Context<Self>) -> R,
    ) -> (R, Vec<SharedString>) {
        let outer = self.data.begin_capture();
        let result = change(self, cx);
        let failures = self.data.end_capture(outer);
        (
            result,
            failures.iter().map(|failure| failure.message()).collect(),
        )
    }
}

/// Write `image`, the database after `changes` changes, unless a newer one
/// is there already.
fn write_image(
    path: &Path,
    image: &[u8],
    changes: u64,
    written: &Mutex<u64>,
) -> std::io::Result<()> {
    let mut written = written.lock().unwrap_or_else(|error| error.into_inner());
    if *written > changes {
        return Ok(());
    }
    shellrs_ssh::files::write(path, image)?;
    *written = changes;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::AuthKind;

    fn draft(name: &str, group: Option<GroupId>) -> HostDraft {
        HostDraft::new(name, "10.0.0.1", 22, "root", AuthKind::Password, group)
    }

    /// The database in memory reaches its file a moment after a change,
    /// whole batches only, and at once when asked.
    #[gpui_kit::test]
    fn the_database_in_memory_is_kept_in_its_file(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::AppContext as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shellrs.db");
        let store = cx.new(|cx| {
            let mut store = HostStore::load(HostDatabase::from_image(None).unwrap()).unwrap();
            store.keep_at(path.clone(), cx);
            store
        });
        let kept = |path: &Path| {
            let image = std::fs::read(path).unwrap();
            let data = HostData::load(HostDatabase::from_image(Some(&image)).unwrap()).unwrap();
            data.hosts()
                .iter()
                .map(|host| host.name.to_string())
                .collect::<Vec<_>>()
        };
        let wait = |cx: &mut gpui_kit::TestAppContext| {
            cx.executor().advance_clock(SAVE_DELAY * 2);
            cx.run_until_parked();
        };

        store.update(cx, |store, cx| store.insert(draft("web", None), cx));
        assert!(!path.exists());
        wait(cx);
        assert_eq!(kept(&path), ["web"]);

        // A batch is written once it has ended, not half of it.
        store.update(cx, |store, cx| {
            store.in_one_transaction(cx, |store, cx| {
                store.insert(draft("db", None), cx);
                store.save_now(cx);
                store.insert(draft("cache", None), cx);
            })
        });
        assert_eq!(kept(&path), ["web"]);
        wait(cx);
        assert_eq!(kept(&path), ["web", "db", "cache"]);

        // Locking and quitting do not wait.
        store.update(cx, |store, cx| {
            store.insert(draft("mail", None), cx);
            store.save_now(cx);
        });
        assert_eq!(kept(&path).len(), 4);
        // Nothing changed: nothing is written.
        std::fs::remove_file(&path).unwrap();
        store.update(cx, |store, cx| store.save_now(cx));
        wait(cx);
        assert!(!path.exists());
    }

    #[gpui_kit::test]
    fn a_locked_store_keeps_its_data_in_the_file_alone(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::AppContext as _;

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shellrs.db");
        let store = cx.new(|cx| {
            let mut store = HostStore::load(HostDatabase::in_memory().unwrap()).unwrap();
            store.keep_at(path.clone(), cx);
            store
        });
        let (web, terminal) = store.update(cx, |store, cx| {
            let web = store.insert(draft("web", None), cx);
            let terminal = store.insert_saved_terminal(None, None, cx).unwrap();
            store.unload(cx);
            (web, terminal)
        });
        store.read_with(cx, |store, _| {
            assert!(store.is_unloaded());
            assert!(store.hosts().is_empty());
            assert!(store.saved_terminal(terminal).is_none());
        });
        // An open terminal goes on: what it says waits.
        store.update(cx, |store, cx| {
            store.set_saved_terminal_directory(terminal, "/var/log".into(), cx);
        });
        cx.run_until_parked();

        store.update(cx, |store, cx| store.reload(cx));
        store.update(cx, |store, cx| {
            assert!(!store.is_unloaded());
            assert_eq!(store.host(web).unwrap().name.as_ref(), "web");
            assert_eq!(
                store.saved_terminal(terminal).unwrap().directory.as_deref(),
                Some("/var/log")
            );
            // Changes after it are written as before.
            store.insert(draft("db", None), cx);
            store.save_now(cx);
        });
        let image = shellrs_ssh::files::read(&path).unwrap();
        let kept = HostDatabase::from_image(Some(&image))
            .unwrap()
            .load()
            .unwrap();
        assert_eq!(kept.hosts.len(), 2);
    }

    #[gpui_kit::test]
    fn a_store_without_a_file_stays_loaded(cx: &mut gpui_kit::TestAppContext) {
        use gpui_kit::AppContext as _;

        let store = cx.new(|_| HostStore::seed());
        store.update(cx, |store, cx| store.unload(cx));
        store.read_with(cx, |store, _| {
            assert!(!store.is_unloaded());
            assert!(!store.hosts().is_empty());
        });
    }

    #[gpui_kit::test]
    fn one_transaction_keeps_every_write_and_reports_a_repeated_failure_once(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::AppContext as _;
        use std::{cell::RefCell, rc::Rc};

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shellrs.db");
        let store = cx.new(|_| HostStore::load(HostDatabase::open(&path).unwrap()).unwrap());
        let events = Rc::new(RefCell::new(Vec::new()));
        let _subscription = cx.update(|cx| {
            let events = events.clone();
            cx.subscribe(&store, move |_, event: &HostStoreEvent, _| {
                events.borrow_mut().push(event.clone());
            })
        });

        let (_, failures) = store.update(cx, |store, cx| {
            store.capture_failures(cx, |store, cx| {
                store.in_one_transaction(cx, |store, cx| {
                    let group = store.insert_group(GroupDraft::new("生产", None), cx);
                    store.insert(draft("web", Some(group)), cx);
                    // Nested: joins the outer transaction.
                    store.in_one_transaction(cx, |store, cx| store.insert(draft("db", None), cx));
                    // Three hosts in a group the database never heard of.
                    for name in ["a", "b", "c"] {
                        store.insert(draft(name, Some(GroupId(99))), cx);
                    }
                })
            })
        });
        assert_eq!(failures.len(), 3, "{failures:?}");
        cx.run_until_parked();
        assert_eq!(
            *events.borrow(),
            [HostStoreEvent::PersistFailed(failures[0].clone())]
        );

        let data = HostDatabase::open(&path).unwrap().load().unwrap();
        let names: Vec<&str> = data.hosts.iter().map(|host| host.name.as_ref()).collect();
        assert_eq!(names, ["web", "db"]);
        assert_eq!(data.groups.len(), 1);
    }

    #[gpui_kit::test]
    fn a_change_says_what_could_not_be_kept_and_still_reports_it(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::AppContext as _;
        use std::{cell::RefCell, rc::Rc};

        let database = HostDatabase::in_memory().unwrap();
        let store = cx.new(|_| HostStore::load(database).unwrap());
        let events = Rc::new(RefCell::new(Vec::new()));
        let _subscription = cx.update(|cx| {
            let events = events.clone();
            cx.subscribe(&store, move |_, event: &HostStoreEvent, _| {
                events.borrow_mut().push(event.clone());
            })
        });

        // A group the database has never heard of: the write fails, the
        // host stands in memory.
        let ((id, kept), failures) = store.update(cx, |store, cx| {
            store.capture_failures(cx, |store, cx| {
                let kept = store.insert(draft("web", None), cx);
                (store.insert(draft("db", Some(GroupId(99))), cx), kept)
            })
        });
        assert_eq!(failures.len(), 1, "{failures:?}");
        assert!(failures[0].starts_with("新建主机未能保存到本地数据库"));
        cx.run_until_parked();
        assert_eq!(
            *events.borrow(),
            [HostStoreEvent::PersistFailed(failures[0].clone())]
        );
        store.read_with(cx, |store, _| {
            assert!(store.host(id).is_some() && store.host(kept).is_some());
        });
        // Nothing left listening afterwards.
        let (_, failures) = store.update(cx, |store, cx| {
            store.capture_failures(cx, |store, cx| store.remove(kept, cx))
        });
        assert!(failures.is_empty());

        // A keychain that keeps nothing: the write says so when it is done.
        let secret = SecretRef::password("root", "10.0.0.1", 22);
        let write = store.update(cx, |store, cx| {
            store.write_secret(secret, Some("hunter2".into()), cx)
        });
        let failure = cx.foreground_executor.block_test(write).unwrap_err();
        assert!(failure.starts_with("密码未能加密保存"), "{failure}");
    }

    #[gpui_kit::test]
    fn a_copy_is_named_after_the_original_in_the_language_it_was_made_in(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use gpui_kit::AppContext as _;

        crate::i18n::isolate_thread();
        let store = cx.new(|_| HostStore::seed());
        let copy = store.update(cx, |store, cx| {
            let web01 = store.hosts()[0].id;
            store.duplicate(web01, cx).unwrap()
        });
        crate::i18n::set_locale("en");
        let english = store.update(cx, |store, cx| {
            let web01 = store.hosts()[0].id;
            store.duplicate(web01, cx).unwrap()
        });
        store.read_with(cx, |store, _| {
            assert_eq!(store.host(copy).unwrap().name.as_ref(), "web-01 副本");
            assert_eq!(store.host(english).unwrap().name.as_ref(), "web-01 copy");
        });
    }
}
