//! The newly opened ShellRS's end: find the one already running and ask it
//! to come forward.

use std::{
    io::{self, BufReader, Write},
    path::Path,
    sync::mpsc,
    time::Duration,
};

use super::link::OpenLink;
use super::protocol::{Request, read_frame, write_json};

/// How long a running app gets to say it heard that it should come
/// forward. One too busy to answer is running all the same.
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(2);

/// Ask the ShellRS listening at `endpoint` to bring its window forward,
/// and to open `open` there; whether there is one. `false` means nothing is
/// running there and the caller is free to start.
///
/// Whatever takes the connection counts as running, whether or not it
/// understands the request: a second copy on the same data would be worse
/// than a window left behind.
pub fn activate_running_app(endpoint: &Path, open: Option<OpenLink>) -> bool {
    let Ok(stream) = connect(endpoint) else {
        return false;
    };
    // On a thread, because a pipe cannot be read with a timeout and an app
    // that hangs must not hang the one being opened.
    let (done, heard) = mpsc::channel();
    let asked = std::thread::Builder::new()
        .name("shellrs-activate".into())
        .spawn(move || {
            let _ = done.send(ask_to_come_forward(&stream, open));
        });
    if asked.is_ok() {
        let _ = heard.recv_timeout(ACTIVATION_TIMEOUT);
    }
    true
}

/// Send the request and wait for the answer, whatever it says: by then the
/// app has the request.
fn ask_to_come_forward<S>(stream: S, open: Option<OpenLink>) -> io::Result<()>
where
    S: Copy + io::Read + Write,
{
    let mut writer = stream;
    write_json(&mut writer, &Request::Activate { open })?;
    read_frame(&mut BufReader::new(stream)).map(|_| ())
}

#[cfg(unix)]
fn connect(socket: &Path) -> io::Result<std::os::unix::net::UnixStream> {
    std::os::unix::net::UnixStream::connect(socket)
}

/// Open the app's pipe, and make sure it is the app's before saying
/// anything: the name is the machine's, so someone else could hold it.
#[cfg(windows)]
fn connect(pipe: &Path) -> io::Result<std::fs::File> {
    use super::pipe_windows;

    let pipe = pipe_windows::open(pipe)?;
    if !pipe_windows::owned_by_current_user(&pipe)? {
        return Err(io::ErrorKind::PermissionDenied.into());
    }
    Ok(pipe)
}

#[cfg(not(any(unix, windows)))]
fn connect(_: &Path) -> io::Result<std::fs::File> {
    Err(io::ErrorKind::Unsupported.into())
}
