//! The running app's end of the instance socket. Listens for as long as
//! the app runs and answers each connection on a thread of its own.

#[cfg(windows)]
use std::sync::atomic::{AtomicBool, Ordering};
use std::{
    io::{self, BufReader},
    path::PathBuf,
    sync::{Arc, Mutex},
    thread::JoinHandle,
};

#[cfg(unix)]
use tokio::sync::watch;

use super::link::OpenLink;
use super::protocol::{Reply, Request, parse_json, read_frame, write_json};
use crate::i18n::t;

/// Set when ShellRS was opened again while this one runs, with the links it
/// was opened with, until the app has come forward and opened them.
type Shared = Mutex<Option<Vec<OpenLink>>>;

/// Listens on the instance socket until dropped.
pub struct InstanceServer {
    shared: Arc<Shared>,
    _listener: Listener,
}

impl InstanceServer {
    /// Take over `endpoint` (from [`crate::app::instance_endpoint`]), which
    /// must not be another running ShellRS's. Only the user running the app
    /// can connect.
    pub fn reserve(endpoint: PathBuf) -> io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let listener = listen(endpoint, shared.clone())?;
        Ok(Self {
            shared,
            _listener: listener,
        })
    }

    /// Whether ShellRS was opened again since this was last asked, and the
    /// links it was opened with, oldest first: `Some` and empty for a plain
    /// re-open. The request threads cannot reach the window, so the app asks
    /// here on a timer, then comes forward and opens them.
    pub fn take_activation(&self) -> Option<Vec<OpenLink>> {
        self.shared
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
    }
}

/// The listening thread, stopped when dropped. On Unix, the socket file
/// goes with it.
#[cfg(unix)]
struct Listener {
    socket: PathBuf,
    shutdown: watch::Sender<bool>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(unix)]
impl Drop for Listener {
    fn drop(&mut self) {
        self.shutdown.send_replace(true);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_file(&self.socket);
    }
}

/// The socket is the user's alone, and every caller's user id is checked
/// again.
#[cfg(unix)]
fn listen(socket: PathBuf, shared: Arc<Shared>) -> io::Result<Listener> {
    use std::os::unix::fs::{MetadataExt as _, PermissionsExt as _};
    use std::os::unix::net::{UnixListener, UnixStream};

    if std::fs::symlink_metadata(&socket).is_ok() {
        if UnixStream::connect(&socket).is_ok() {
            return Err(another_app());
        }
        // Left behind by a ShellRS that did not get to clean up.
        std::fs::remove_file(&socket)?;
    }
    if let Some(dir) = socket.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(&socket)?;
    std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
    let owner = std::fs::metadata(&socket)?.uid();
    listener.set_nonblocking(true)?;

    let (shutdown, mut stop) = watch::channel(false);
    let thread = std::thread::Builder::new()
        .name("shellrs-instance".into())
        .spawn(move || {
            let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            else {
                return;
            };
            runtime.block_on(async move {
                let Ok(listener) = tokio::net::UnixListener::from_std(listener) else {
                    return;
                };
                loop {
                    tokio::select! {
                        accepted = listener.accept() => {
                            if let Ok((stream, _)) = accepted {
                                accept(stream, owner, &shared);
                            }
                        }
                        _ = stop.changed() => break,
                    }
                }
            });
        })?;
    Ok(Listener {
        socket,
        shutdown,
        thread: Some(thread),
    })
}

/// Hand a connection from the same user to a thread of its own.
#[cfg(unix)]
fn accept(stream: tokio::net::UnixStream, owner: u32, shared: &Arc<Shared>) {
    if !stream
        .peer_cred()
        .is_ok_and(|credentials| credentials.uid() == owner)
    {
        return;
    }
    let Ok(stream) = stream.into_std() else {
        return;
    };
    if stream.set_nonblocking(false).is_err() {
        return;
    }
    let shared = shared.clone();
    let _ = std::thread::Builder::new()
        .name("shellrs-instance-request".into())
        .spawn(move || {
            // A caller that hangs up mid-answer is no one's problem.
            let _ = serve(&mut BufReader::new(&stream), &mut &stream, &shared);
        });
}

/// The listening thread, stopped when dropped.
#[cfg(windows)]
struct Listener {
    pipe: PathBuf,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(windows)]
impl Drop for Listener {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        let Some(thread) = self.thread.take() else {
            return;
        };
        // The thread waits in ConnectNamedPipe; a caller wakes it. Until it
        // has gone: between two pipe instances there is nothing to connect
        // to, so one knock may not be heard.
        while !thread.is_finished() {
            let _ = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.pipe);
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        let _ = thread.join();
    }
}

/// A named pipe only the current user can open, and only from this
/// machine. Its name is the machine's, not a file of the user's, so it is
/// created as the first instance: if anyone already holds it, the server
/// does not start.
#[cfg(windows)]
fn listen(pipe: PathBuf, shared: Arc<Shared>) -> io::Result<Listener> {
    use super::pipe_windows;

    let first = pipe_windows::create_instance(&pipe, true).map_err(|error| {
        if pipe_windows::is_taken(&error) {
            another_app()
        } else {
            error
        }
    })?;
    let stop = Arc::new(AtomicBool::new(false));
    let thread = std::thread::Builder::new()
        .name("shellrs-instance".into())
        .spawn({
            let (pipe, stop) = (pipe.clone(), stop.clone());
            move || {
                let mut waiting = first;
                loop {
                    let connected = pipe_windows::wait_for_client(&waiting);
                    if stop.load(Ordering::Acquire) {
                        break;
                    }
                    // The next instance first, so a caller arriving now
                    // finds the pipe busy rather than gone.
                    let next = loop {
                        match pipe_windows::create_instance(&pipe, false) {
                            Ok(next) => break Some(next),
                            Err(_) if !stop.load(Ordering::Acquire) => {
                                std::thread::sleep(std::time::Duration::from_millis(100))
                            }
                            Err(_) => break None,
                        }
                    };
                    let Some(next) = next else {
                        break;
                    };
                    let current = std::mem::replace(&mut waiting, next);
                    // Otherwise the caller came and went already.
                    if connected.is_ok() {
                        answer(std::fs::File::from(current), &shared);
                    }
                }
            }
        })?;
    Ok(Listener {
        pipe,
        stop,
        thread: Some(thread),
    })
}

/// Answer a connected caller on a thread of its own. Closing the pipe
/// afterwards keeps what the caller has not read yet: only
/// `DisconnectNamedPipe` would throw it away.
#[cfg(windows)]
fn answer(pipe: std::fs::File, shared: &Arc<Shared>) {
    let shared = shared.clone();
    let _ = std::thread::Builder::new()
        .name("shellrs-instance-request".into())
        .spawn(move || {
            // A caller that hangs up mid-answer is no one's problem.
            let _ = serve(&mut BufReader::new(&pipe), &mut &pipe, &shared);
        });
}

#[cfg(not(any(unix, windows)))]
struct Listener;

#[cfg(not(any(unix, windows)))]
fn listen(_: PathBuf, _: Arc<Shared>) -> io::Result<Listener> {
    Err(io::ErrorKind::Unsupported.into())
}

fn another_app() -> io::Error {
    io::Error::new(
        io::ErrorKind::AddrInUse,
        t!("instance.another_app").to_string(),
    )
}

/// Read one request and answer it.
fn serve(
    reader: &mut impl io::Read,
    writer: &mut impl io::Write,
    shared: &Shared,
) -> io::Result<()> {
    let Some(payload) = read_frame(reader)? else {
        return Ok(());
    };
    let reply = match parse_json::<Request>(&payload) {
        Ok(Request::Activate { open }) => {
            shared
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .get_or_insert_with(Vec::new)
                .extend(open);
            Reply::Activated
        }
        Err(error) => Reply::Error {
            message: error.to_string(),
        },
    };
    write_json(writer, &reply)
}

#[cfg(test)]
mod tests {
    use super::super::activate_running_app;
    use super::*;

    #[test]
    fn opened_again_the_running_app_hears_it_and_gets_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let endpoint = crate::app::instance_endpoint(dir.path());
        assert!(!activate_running_app(&endpoint, None));

        let server = InstanceServer::reserve(endpoint.clone()).unwrap();
        assert_eq!(server.take_activation(), None);
        // A second ShellRS on the same data is refused.
        let second = InstanceServer::reserve(endpoint.clone()).err().unwrap();
        assert_eq!(second.kind(), io::ErrorKind::AddrInUse);

        assert!(activate_running_app(&endpoint, None));
        assert_eq!(server.take_activation(), Some(Vec::new()));
        let link = OpenLink {
            url: "ssh://root@10.0.0.9".into(),
            tab: None,
        };
        assert!(activate_running_app(&endpoint, Some(link.clone())));
        assert_eq!(server.take_activation(), Some(vec![link]));
        assert_eq!(server.take_activation(), None);

        // The endpoint goes with the server.
        drop(server);
        assert!(!activate_running_app(&endpoint, None));
        // A socket file left behind is taken over.
        #[cfg(unix)]
        std::fs::write(&endpoint, "").unwrap();
        InstanceServer::reserve(endpoint).unwrap();
    }
}
