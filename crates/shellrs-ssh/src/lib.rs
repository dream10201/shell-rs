//! ShellRS's SSH core: logging in (host keys, passwords, keys, the agent,
//! jump hosts and proxies), the channels of a live connection (a shell on
//! a pseudo-terminal, commands) and copying files through the host's shell.
//!
//! It shows nothing and says nothing in words: questions for the user come
//! out as typed prompts, failures as typed errors in `anyhow` chains, and
//! the app words them in its own language. Secrets are read through [`SecretSource`],
//! only when a server asks for one, so the app keeps them in its own
//! keychain.

mod connection;
mod error;
pub mod files;
mod latency;
mod login;
#[cfg(target_vendor = "apple")]
mod nat64;
mod probe;
mod prompt;
mod proxy;
mod secret;
mod session;
pub mod transfer;

pub use connection::{
    AgentLocation, ForwardedTcpip, SshConnectionConfig, SshConnector, SshHandle, SshPrompts,
    Tunnel, is_network_error, timeout_excluding_prompts,
};
pub use error::{
    AgentProblem, AgentUnreachable, AuthError, ChannelError, ConnectError, ForwardProblem,
    KnownHostsError, MissingCredential, PasswordWanted, ProxyError, RouteFailure, WrapErr, Wrapped,
    find_cause, link_as, wrap, wrap_shared,
};
pub use latency::{LATENCY_INTERVAL, Latency, LatencyLevel, round_trip};
pub use login::{
    DEFAULT_KEY_FILES, HostLogin, JumpLogin, LoginMethod, LoginRoute, ProxyKind, ProxyLogin,
};
pub use probe::{HOST_OS_KEYS, HostOsProbe, PROBE_COMMAND, ProbeOutcome, WINDOWS_PROBE_COMMAND};
pub use prompt::{
    AuthenticationPrompt, AuthenticationRequest, ConnectionPrompt, ConnectionPromptField,
    ConnectionPromptKind, ConnectionPromptReply, ConnectionSecret, HostKeyChangedPrompt,
    UnknownHostPrompt,
};
pub use secret::{SecretRef, SecretSource, SharedSecretSource, read_secret};
pub use session::{
    ChannelStream, PtySize, open_command_channel, open_probe, open_shell, resize_pty, run_exec,
    write_shell,
};
