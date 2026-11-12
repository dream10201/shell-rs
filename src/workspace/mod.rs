//! The window shell: title bar, dock area with its sidebars, the switch of
//! the right sidebar's tools, status bar, and the start page (recent hosts)
//! the center shows while no tab is open.

mod app_lock;
mod app_shell;
mod compose;
mod compose_history;
mod compose_pane;
mod credentials;
mod dock_skin;
mod forwards;
mod front;
mod imports;
mod links;
mod projects;
mod recent_hosts;
mod sidebar;
mod sidebar_widths;
mod snippets;
mod status_bar;
mod sync;
mod tabs;
mod terminal_groups;
mod title_bar;
mod tmux;
mod tool_sidebar;
mod tools;
mod transfers;
mod updates;
mod window_state;
mod workspace_view;
mod zmodem;

pub use links::open_link_once_open;
pub use window_state::WindowState;
pub use workspace_view::{Workspace, notify_once_open, window_options};

pub use app_shell::{AppShell, StartupFailure};
