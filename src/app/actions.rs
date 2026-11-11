//! Every user command, so toolbar buttons, menus and key bindings dispatch
//! the same action and cannot disagree.

use std::path::PathBuf;

use gpui_kit::component::Icon;
use gpui_kit::*;
use shellrs_sync::DeviceId;

use super::CatalogIcon;
use crate::i18n::t;
use crate::{
    host::{
        CredentialId, ForwardId, GroupId, HostId, HostNode, NodeDrop, SavedTerminalId,
        SnippetCategoryId, SnippetDrop, SnippetId, SnippetNode, TerminalDrop, TerminalGroupId,
        TerminalNode,
    },
    monitor::MonitorDetail,
    processes::ProcessSort,
    services::ServiceCommand,
    terminal::{LocalTerminalId, RemoteTerminalId},
};

gpui_kit::actions!(
    shellrs,
    [
        /// Open the new-host dialog for a host at the root of the tree.
        NewHost,
        /// Open the 临时连接 dialog: connect to a host without saving it.
        NewTemporaryConnection,
        /// Open 快速连接: search the saved hosts and connect to one or
        /// several.
        QuickConnect,
        /// Open the new-group dialog for a top-level group.
        NewGroup,
        /// Open the import dialog for the hosts `~/.ssh/config` names.
        ImportSshConfig,
        /// Open the import dialog for the hosts `~/.ssh/known_hosts` names.
        ImportKnownHosts,
        /// Open a new local login-shell terminal.
        NewLocalTerminal,
        /// Start 命令广播, which turns terminal tab marks into checkboxes,
        /// or end it.
        ToggleTerminalBroadcast,
        /// Show or hide 撰写窗格, the compose pane below the terminals.
        ToggleComposePane,
        /// Run what 撰写窗格 holds in the terminal in front, or in every
        /// terminal of 命令广播.
        SendCompose,
        /// Send Ctrl+C where 撰写窗格 sends.
        SendComposeInterrupt,
        /// Whether typing in one terminal of 命令广播 goes to the others too.
        ToggleBroadcastKeys,
        /// Copy the active terminal selection.
        CopyTerminal,
        /// Paste the clipboard into the active terminal.
        PasteTerminal,
        /// Select all of the active terminal's text, the scrollback with it.
        SelectAllTerminal,
        /// Open the active terminal's find bar, or select its query again.
        FindInTerminal,
        /// Focus the next match down (newer output) in the active terminal.
        FindNextInTerminal,
        /// Focus the next match up (older output) in the active terminal.
        FindPreviousInTerminal,
        /// Close the active terminal's find bar and its highlights.
        DismissTerminalFind,
        /// Clear the active terminal's screen and scrollback, keeping the
        /// prompt line.
        ClearTerminal,
        /// Show or hide the left dock, whichever list it is showing.
        ToggleHostPanel,
        /// Show or hide the right sidebar, whichever tool it is showing.
        ToggleToolSidebar,
        /// Fill the screen with the window, as double-clicking its title
        /// bar does.
        MaximizeWindow,
        /// Put a maximized window back to its size before.
        RestoreWindow,
        /// 显示或隐藏 ShellRS: bring it in front of other apps, or put it
        /// away when it already is. The system tells of its key
        /// (`system_hotkey`), not the keymap.
        ShowOrHideApp,
        /// Restore the main window, including from the notification area.
        ShowApp,
        /// 关于 ShellRS in the macOS menu bar: the system's about panel.
        AboutApp,
        /// 隐藏 ShellRS, ⌘H on macOS: away until the Dock icon or ⌘Tab
        /// brings it back.
        HideApp,
        /// 隐藏其他, ⌥⌘H on macOS: hide every other app.
        HideOtherApps,
        /// 全部显示 in the macOS menu bar: show the apps that were hidden.
        ShowAllApps,
        /// Minimize the window in front, ⌘M on macOS.
        MinimizeWindow,
        /// 缩放 in the macOS menu bar: fill the screen with the window in
        /// front, or put it back to its size before.
        ZoomWindow,
        /// Read the network connections of the SSH terminal's host again.
        RefreshConnections,
        /// Read the processes of the SSH terminal's host again.
        RefreshProcesses,
        /// Read the services of the SSH terminal's host again.
        RefreshServices,
        /// Read the bash history of the SSH terminal's host again.
        RefreshHistory,
        /// Read the tmux sessions of the SSH terminal's host again.
        RefreshTmux,
        /// Open the dialog that names a new tmux session, then start it in
        /// the SSH terminal in front.
        NewTmuxSession,
        /// Detach the SSH terminal in front from its tmux session, leaving
        /// the session running.
        DetachTmux,
        /// Open the new-snippet dialog, for a snippet at the top, in no
        /// category.
        NewSnippet,
        /// Open the new-category dialog of 命令片段, for a category at the
        /// top.
        NewSnippetCategory,
        /// Open the new-group dialog of 项目.
        NewTerminalGroup,
        /// Show the host list in the left dock.
        ShowHosts,
        /// Show the port-forwarding list in the left dock.
        ShowForwards,
        /// Open the new-forward dialog.
        NewForward,
        /// Start the forward selected in the forward list, or stop it if it
        /// is running.
        ToggleSelectedForward,
        /// Move the forward list's selection to the row above.
        SelectPreviousForward,
        /// Move the forward list's selection to the row below.
        SelectNextForward,
        /// Show the credential list in the left dock.
        ShowCredentials,
        /// Open the new-credential dialog.
        NewCredential,
        /// Open the new-credential dialog at a key it generates.
        GenerateCredentialKey,
        /// Open the edit dialog of the credential selected in the credential
        /// list.
        EditSelectedCredential,
        /// Move the credential list's selection to the row above.
        SelectPreviousCredential,
        /// Move the credential list's selection to the row below.
        SelectNextCredential,
        /// Switch between the light and dark theme.
        ToggleTheme,
        /// Move keyboard focus to the search field of the list the left dock
        /// is showing.
        FocusSearch,
        /// Connect the host selected in the focused host list.
        ConnectSelected,
        /// Expand every group in the host tree.
        ExpandAllGroups,
        /// Collapse every group in the host tree.
        CollapseAllGroups,
        /// Close the center tab displayed most recently (the one ⌘W acts on).
        CloseActiveTab,
        /// Show the tab after the current one in its group, the first after
        /// the last.
        NextTab,
        /// Show the tab before the current one in its group, the last before
        /// the first.
        PreviousTab,
        /// Open the settings tab, or bring it forward if it is already open.
        OpenSettings,
        /// Change the application password after verifying the current one.
        ChangeAppLockPassword,
        /// Hide the workspace until the application password is entered.
        LockApp,
        /// Close the settings tab.
        CloseSettings,
        /// Increase the application base font (interface zoom).
        ZoomIn,
        /// Decrease the application base font (interface zoom).
        ZoomOut,
        /// Reset the application base font.
        ZoomReset,
        /// Make the terminals' text one size larger (设置 › 终端 › 字号).
        ZoomTerminalIn,
        /// Make the terminals' text one size smaller.
        ZoomTerminalOut,
        /// Put the terminals' text back to its default size.
        ZoomTerminalReset,
        /// Quit the application.
        Quit,
        /// Stop the file transfers that are running.
        CancelFileTransfers,
        /// Look for a newer ShellRS now.
        CheckForUpdates,
        /// Download the newer ShellRS that was found.
        DownloadUpdate,
        /// Show the newer ShellRS: restarting into it, and where to read
        /// what changed.
        ShowUpdate,
        /// Restart into the downloaded ShellRS.
        RestartToUpdate,
        /// Open the page to download ShellRS by hand.
        OpenDownloadPage,
        /// Open the website's changelog.
        OpenChangelog,
        /// Open the ShellRS website.
        OpenWebsite,
        /// 开启同步 › 选择文件夹…: pick the folder to sync through, then make
        /// sync there or join it.
        SetUpSync,
        /// 开启同步 › 连接 WebDAV…: name the server and the login, then make
        /// sync there or join it.
        SetUpWebDavSync,
        /// 开启同步 › 连接 S3…: name the bucket and its keys, then make sync
        /// there or join it.
        SetUpS3Sync,
        /// Sign in anew to the WebDAV server or bucket sync goes through.
        ChangeSyncLogin,
        /// Run a round of sync now.
        SyncNow,
        /// Open what sync waits for the user to settle: a password, a
        /// reset, deletions to agree to.
        ReviewSync,
        /// Open 更换同步密码, which changes the data key too.
        ChangeSyncPassword,
        /// Open 关闭同步.
        TurnOffSync,
    ]
);

/// Commands that carry the id of what they act on.
macro_rules! id_actions {
    ($($(#[$doc:meta])* $name:ident($id:ty);)*) => {
        $(
            $(#[$doc])*
            #[derive(Action, Clone, PartialEq, Eq)]
            #[action(namespace = shellrs, no_json)]
            pub struct $name(pub $id);
        )*
    };
}

id_actions! {
    /// Open a new terminal connection for a host.
    ConnectHost(HostId);
    /// Open the edit-host dialog for a host.
    EditHost(HostId);
    /// Choose files of this computer to copy to a remote terminal's host.
    UploadFiles(RemoteTerminalId);
    /// Ask what to copy here from a remote terminal's host.
    DownloadFiles(RemoteTerminalId);
    /// Duplicate a host in the same group.
    DuplicateHost(HostId);
    /// Ask for confirmation, then delete a host.
    DeleteHost(HostId);
    /// Copy a host's address (an IP address or a host name) to the
    /// clipboard.
    CopyHostAddress(HostId);
    /// Copy a host's public id to the clipboard, for another tool to
    /// name the machine by.
    CopyHostId(HostId);

    /// Open a terminal connection for every host in this group's subtree.
    ConnectGroup(GroupId);
    /// Open the new-host dialog with this group pre-selected.
    NewHostInGroup(GroupId);
    /// Open the new-group dialog for a group nested inside this one.
    NewChildGroup(GroupId);
    /// Open the rename-group dialog.
    RenameGroup(GroupId);
    /// Ask for confirmation, then delete a group with everything inside it.
    DeleteGroup(GroupId);

    /// Close one remote terminal connection and its tab.
    CloseTerminal(RemoteTerminalId);
    /// Reconnect one remote terminal using the host's latest settings.
    ReconnectTerminal(RemoteTerminalId);
    /// Disconnect one remote terminal, keeping its tab to reconnect from.
    DisconnectTerminal(RemoteTerminalId);
    /// Open the dialog that gives one remote terminal tab its own title.
    RenameTerminal(RemoteTerminalId);
    /// Copy an open terminal's connection, tab title and working directory.
    DuplicateTerminal(RemoteTerminalId);
    /// Hold a serial host's line in the break condition for a moment.
    SendBreak(RemoteTerminalId);

    /// Close a local terminal tab and terminate its child process.
    CloseLocalTerminal(LocalTerminalId);
    /// Restart a local terminal with a fresh emulator and PTY.
    RestartLocalTerminal(LocalTerminalId);

    /// Make sync in this folder, or join the sync there: what the folder
    /// picker of 开启同步… chose.
    SetUpSyncIn(PathBuf);
    /// Ask first, then take a device out of sync: its file in the folder
    /// goes.
    RemoveSyncDevice(DeviceId);

    /// Show the tab at a position, from 1, in the current tab's group; 9 is
    /// the last, as in browsers.
    SwitchToTab(usize);

    /// Show a tool in the right sidebar, or hide the sidebar if it is
    /// already showing that tool.
    ToggleTool(ToolKind);
    /// Unfold a part of the system monitor, or fold it away again.
    ToggleMonitorDetail(MonitorDetail);
    /// Attach the SSH terminal in front to a tmux session of its host, by
    /// the session's id.
    AttachTmuxSession(String);
    /// Open the dialog that renames a tmux session, by its id.
    RenameTmuxSession(String);
    /// Ask, then end a tmux session and the programs in it, by its id.
    KillTmuxSession(String);
    /// Sort the process list by memory or CPU, the most first; by the same
    /// again, the other way.
    SortProcesses(ProcessSort);
    /// Open the details of a process of the host of the SSH terminal in
    /// front, by its PID.
    ShowProcess(u32);
    /// Open the details of a service of the host of the SSH terminal in
    /// front, by its name.
    ShowService(String);
    /// Copy a command of 历史命令 or 命令片段 to the clipboard.
    CopyCommand(String);
    /// Open the new-snippet dialog with this category picked.
    NewSnippetIn(SnippetCategoryId);
    /// Open the edit-snippet dialog.
    EditSnippet(SnippetId);
    /// Ask for confirmation, then delete a snippet.
    DeleteSnippet(SnippetId);
    /// Send a snippet where 撰写窗格 sends: the terminal in front, or every
    /// terminal of 命令广播.
    SendComposeSnippet(SnippetId);
    /// Open the new-category dialog of 命令片段, for a category in this
    /// one.
    NewSnippetSubcategory(SnippetCategoryId);
    /// Open the rename dialog of a snippet category, which can also move
    /// it into another.
    RenameSnippetCategory(SnippetCategoryId);
    /// Ask for confirmation, then delete a snippet category with the
    /// categories inside it and every snippet in them.
    DeleteSnippetCategory(SnippetCategoryId);
    /// Fold a category of 命令片段 away, or unfold it.
    ToggleSnippetCategory(SnippetCategoryId);

    /// Keep a new local terminal in 项目, in a group or at the top, and
    /// open it.
    NewSavedTerminal(Option<TerminalGroupId>);
    /// Hide a group's terminals in 项目, or show them again.
    ToggleTerminalGroup(TerminalGroupId);
    /// Open the rename dialog of a group of 项目.
    RenameTerminalGroup(TerminalGroupId);
    /// Ask for confirmation, then delete a group of 项目 with its
    /// terminals; their open tabs stay.
    DeleteTerminalGroup(TerminalGroupId);
    /// Bring a saved terminal's tab to the front, or open it in the folder
    /// it was last in.
    OpenSavedTerminal(SavedTerminalId);
    /// Close a saved terminal's tab; it stays in 项目.
    CloseSavedTerminal(SavedTerminalId);
    /// Open the rename dialog of a saved terminal.
    RenameSavedTerminal(SavedTerminalId);
    /// Ask for confirmation, then delete a saved terminal; its open tab
    /// stays.
    DeleteSavedTerminal(SavedTerminalId);

    /// Start a port forward over a connection of its own.
    StartForward(ForwardId);
    /// Stop a running port forward and log out its connection.
    StopForward(ForwardId);
    /// Open the edit-forward dialog.
    EditForward(ForwardId);
    /// Ask for confirmation, then delete a port-forwarding rule.
    DeleteForward(ForwardId);

    /// Open the edit-credential dialog.
    EditCredential(CredentialId);
    /// Copy the public half of a key credential's key, for a server's
    /// `authorized_keys`.
    CopyCredentialPublicKey(CredentialId);
    /// Ask for confirmation, then delete a credential. The hosts using it
    /// go back to logging in on their own.
    DeleteCredential(CredentialId);
}

/// Move a host-tree row by dropping it beside a peer or into a group.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct MoveHostNode {
    pub source: HostNode,
    pub destination: NodeDrop,
}

/// A row of 命令片段 dropped where it goes: a category beside another or
/// into one, a snippet beside another or into a category.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct MoveSnippetNode {
    pub source: SnippetNode,
    pub destination: SnippetDrop,
}

/// A row of 项目 dropped where it goes: a group among the groups, a
/// terminal among a group's terminals.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct MoveTerminalNode {
    pub source: TerminalNode,
    pub destination: TerminalDrop,
}

/// Ask for confirmation, then end a process of the host of the SSH terminal
/// in front: with SIGTERM, or with `force` SIGKILL.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct EndProcess {
    pub pid: u32,
    pub force: bool,
}

/// Start, stop or restart a service of the host of the SSH terminal in
/// front, or enable or disable it at boot; asks first before stopping or
/// restarting it.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct ControlService {
    pub name: String,
    pub command: ServiceCommand,
}

/// Put a command on the input line of the terminal the right sidebar goes
/// with, an SSH terminal or a local one, in place of what is typed there,
/// and with `run` run it.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct EnterCommand {
    pub command: String,
    pub run: bool,
}

/// Copy files and folders of this computer into a folder of a remote
/// terminal's host, over the terminal's connection.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct StartUpload {
    pub terminal: RemoteTerminalId,
    pub sources: Vec<PathBuf>,
    pub destination: String,
}

/// Copy a file or folder of a remote terminal's host into a folder of this
/// computer, over the terminal's connection.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct StartDownload {
    pub terminal: RemoteTerminalId,
    pub source: String,
    pub into: PathBuf,
}

/// One tab of the center area, by the identity of what it shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CenterTab {
    Terminal(RemoteTerminalId),
    LocalTerminal(LocalTerminalId),
    /// There is at most one settings tab.
    Settings,
}

/// Select one connected terminal for interactive input broadcast.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct SetBroadcastTarget {
    tab: CenterTab,
    selected: bool,
}

impl SetBroadcastTarget {
    pub fn new(tab: CenterTab, selected: bool) -> Self {
        Self { tab, selected }
    }

    pub fn tab(&self) -> CenterTab {
        self.tab
    }

    pub fn is_selected(&self) -> bool {
        self.selected
    }
}

/// A tool of the right sidebar, in the order of the switch beside it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ToolKind {
    #[default]
    Snippets,
    History,
    Tmux,
    Services,
    Processes,
    Connections,
    Monitor,
    /// 项目, the one tool of local terminals.
    Projects,
}

impl ToolKind {
    pub const ALL: [ToolKind; 8] = [
        ToolKind::Snippets,
        ToolKind::History,
        ToolKind::Tmux,
        ToolKind::Services,
        ToolKind::Processes,
        ToolKind::Connections,
        ToolKind::Monitor,
        ToolKind::Projects,
    ];

    /// What `settings.json` calls the tool in 侧栏功能, and the database
    /// in a host's `sidebar_tool`. Never changed once released: a renamed
    /// key would bring a hidden tool back, and lose the tool a host keeps.
    pub fn key(self) -> &'static str {
        match self {
            ToolKind::Snippets => "snippets",
            ToolKind::History => "history",
            ToolKind::Tmux => "tmux",
            ToolKind::Services => "services",
            ToolKind::Processes => "processes",
            ToolKind::Connections => "connections",
            ToolKind::Monitor => "monitor",
            ToolKind::Projects => "projects",
        }
    }

    /// The tool `key` names; none for a key no tool has.
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tool| tool.key() == key)
    }

    pub fn label(self) -> SharedString {
        match self {
            ToolKind::Snippets => t!("tools.kind.snippets"),
            ToolKind::History => t!("tools.kind.history"),
            ToolKind::Tmux => "tmux".into(),
            ToolKind::Services => t!("tools.kind.services"),
            ToolKind::Processes => t!("tools.kind.processes"),
            ToolKind::Connections => t!("tools.kind.connections"),
            ToolKind::Monitor => t!("tools.kind.monitor"),
            ToolKind::Projects => t!("tools.kind.projects"),
        }
    }

    pub fn icon(self) -> Icon {
        Icon::new(match self {
            ToolKind::Snippets => CatalogIcon::CodeXml,
            ToolKind::History => CatalogIcon::RotateCcwClock,
            ToolKind::Tmux => CatalogIcon::Layers,
            ToolKind::Services => CatalogIcon::ServerCog,
            ToolKind::Processes => CatalogIcon::ListFilter,
            ToolKind::Connections => CatalogIcon::Network,
            ToolKind::Monitor => CatalogIcon::Activity,
            ToolKind::Projects => CatalogIcon::FolderCode,
        })
    }
}

/// Which tabs of a tab bar a batch close takes, relative to one tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CloseScope {
    Left,
    Right,
    Others,
    All,
}

impl CloseScope {
    /// The indexes to close in a bar of `len` tabs, relative to the tab at
    /// `ix`. Empty when there is nothing to close, which is also how a menu
    /// decides to disable the command.
    pub fn targets(self, len: usize, ix: usize) -> Vec<usize> {
        match self {
            CloseScope::Left => (0..ix.min(len)).collect(),
            CloseScope::Right => (ix + 1..len).collect(),
            CloseScope::Others => (0..len).filter(|&other| other != ix).collect(),
            CloseScope::All => (0..len).collect(),
        }
    }
}

/// Close several tabs of the tab bar that holds `tab`, each through its own
/// close path.
#[derive(Action, Clone, PartialEq, Eq)]
#[action(namespace = shellrs, no_json)]
pub struct CloseTabs {
    pub tab: CenterTab,
    pub scope: CloseScope,
}

#[cfg(test)]
mod tests {
    use super::CloseScope;

    #[test]
    fn close_scopes_are_relative_to_the_anchor_tab() {
        assert_eq!(CloseScope::Left.targets(4, 2), vec![0, 1]);
        assert_eq!(CloseScope::Right.targets(4, 2), vec![3]);
        assert_eq!(CloseScope::Others.targets(4, 2), vec![0, 1, 3]);
        assert_eq!(CloseScope::All.targets(4, 2), vec![0, 1, 2, 3]);
    }

    #[test]
    fn close_scopes_are_empty_at_the_edges() {
        assert!(CloseScope::Left.targets(3, 0).is_empty());
        assert!(CloseScope::Right.targets(3, 2).is_empty());
        assert!(CloseScope::Others.targets(1, 0).is_empty());
        assert_eq!(CloseScope::All.targets(1, 0), vec![0]);
    }
}
