//! Copying files to and from a host with nothing but its shell: each step
//! is a command run on a channel of a live connection, a file's bytes going
//! to `cat` or coming from it. Needs no SFTP and no `scp` on the server,
//! only a POSIX `sh` with `cat`, `wc`, `mkdir`, `mv`, `rm` and `find`.
//!
//! Every command is one line, `sh -c '<script>' sh <arguments>`, so
//! whatever the login shell is it hands the script to `sh` untouched: the
//! scripts hold no single quote and no `!`. Paths go as arguments, never
//! inside a script.
//!
//! An upload goes to a file beside its destination and is renamed over it
//! once every byte has arrived, so a copy that is cut short leaves what was
//! there as it was. Folders are copied with what they hold; what is already
//! there under the same name is written over. Links on the host are not
//! followed into.

use std::{
    error::Error,
    fmt,
    path::{Path, PathBuf},
};

use anyhow::{Result, anyhow};
use futures::future::BoxFuture;
use russh::{Channel, ChannelMsg, client};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::watch,
};

use super::error::wrap;

/// How much of a file goes at a time.
const CHUNK: usize = 64 * 1024;

/// How much of what a command says on standard error is kept.
const STDERR_LIMIT: usize = 4 * 1024;

/// What the temporary file of an upload is called after its destination.
const PART_SUFFIX: &str = ".shellrs-part";

/// `p` is the first argument, with a leading `~` as the login folder.
const PATH: &str = r#"p=$1; case $p in "~") p=$HOME;; "~/"*) p=$HOME/${p#"~/"};; esac; "#;

/// Runs commands on the host a copy is with, one channel each.
pub trait CommandRunner: Send {
    fn start(&mut self, command: String) -> BoxFuture<'_, Result<Box<dyn RunningCommand>>>;
}

/// A command started by a [`CommandRunner`].
pub trait RunningCommand: Send {
    /// Give `bytes` to its standard input.
    fn write<'a>(&'a mut self, bytes: &'a [u8]) -> BoxFuture<'a, Result<()>>;
    /// End its standard input.
    fn close_input(&mut self) -> BoxFuture<'_, Result<()>>;
    /// The next of what it prints on standard output, `None` once it has
    /// ended.
    fn read(&mut self) -> BoxFuture<'_, Result<Option<Vec<u8>>>>;
    /// How it ended, asked for after `read` gave `None`.
    fn finish(self: Box<Self>) -> Finished;
}

/// How a command ended: its exit code, when the host gave one, and what it
/// said on standard error.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Finished {
    pub code: Option<u32>,
    pub stderr: String,
}

/// Why a copy did not finish.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferFailure {
    /// A command on the host failed; what it said, which may be nothing.
    Remote(String),
    /// Nothing is at this path of the host.
    NotFound(String),
    /// The host's answer is not what its commands print.
    Unexpected,
    /// A name on the host would land outside the folder asked for.
    UnsafeName(String),
    /// A local name that cannot be made on the host.
    UnsendableName(PathBuf),
    /// Over the error reading or writing this local path.
    Local(PathBuf),
    /// Over the connection's error.
    Connection,
    Cancelled,
}

impl fmt::Display for TransferFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Remote(message) if message.is_empty() => f.write_str("the host's command failed"),
            Self::Remote(message) => write!(f, "the host said: {message}"),
            Self::NotFound(path) => write!(f, "{path} isn't on the host"),
            Self::Unexpected => f.write_str("the host's answer wasn't understood"),
            Self::UnsafeName(name) => write!(f, "the host has an unsafe name: {name}"),
            Self::UnsendableName(path) => write!(f, "{} can't be sent", path.display()),
            Self::Local(path) => write!(f, "couldn't read or write {}", path.display()),
            Self::Connection => f.write_str("the connection failed"),
            Self::Cancelled => f.write_str("cancelled"),
        }
    }
}

impl Error for TransferFailure {}

/// How far a copy has come.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TransferProgress {
    /// The file being copied.
    pub name: String,
    /// Its bytes copied so far, and its size.
    pub done: u64,
    pub total: u64,
    /// Files finished before it.
    pub files: u64,
}

/// What a finished copy moved.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TransferSummary {
    pub files: u64,
    pub bytes: u64,
}

/// `script` with `arguments`, as the one line a login shell is given.
fn command(script: &str, arguments: &[&str]) -> String {
    debug_assert!(!script.contains(['\'', '!', '\n']));
    let mut line = format!("sh -c '{script}' sh");
    for argument in arguments {
        line.push_str(" '");
        line.push_str(&argument.replace('\'', "'\\''"));
        line.push('\'');
    }
    line
}

/// `name` in the host's folder `folder`.
fn remote_join(folder: &str, name: &str) -> String {
    match folder {
        "" => name.to_string(),
        folder if folder.ends_with('/') => format!("{folder}{name}"),
        folder => format!("{folder}/{name}"),
    }
}

fn connection(error: impl Into<anyhow::Error>) -> anyhow::Error {
    wrap(TransferFailure::Connection, error)
}

fn local(path: &Path) -> impl FnOnce(std::io::Error) -> anyhow::Error + '_ {
    move |error| wrap(TransferFailure::Local(path.to_path_buf()), error)
}

/// Copy `sources`, files and folders of this computer, into the host's
/// folder `destination`, made when it is not there. `progress` hears of
/// every chunk; `cancel` turning true stops the copy.
pub async fn upload(
    runner: &mut dyn CommandRunner,
    sources: &[PathBuf],
    destination: &str,
    progress: &mut (dyn FnMut(TransferProgress) + Send),
    cancel: &watch::Receiver<bool>,
) -> Result<TransferSummary> {
    let destination = match destination.trim() {
        "" => "~",
        destination => destination,
    };
    let mut copy = Copy {
        runner,
        progress,
        cancel,
        summary: TransferSummary::default(),
    };
    copy.make_folder(destination).await?;
    for source in sources {
        copy.send(source, destination).await?;
    }
    Ok(copy.summary)
}

/// Copy the host's file or folder `source` into the folder `into` of this
/// computer.
pub async fn download(
    runner: &mut dyn CommandRunner,
    source: &str,
    into: &Path,
    progress: &mut (dyn FnMut(TransferProgress) + Send),
    cancel: &watch::Receiver<bool>,
) -> Result<TransferSummary> {
    let source = source.trim();
    let mut copy = Copy {
        runner,
        progress,
        cancel,
        summary: TransferSummary::default(),
    };
    copy.receive(source, into).await?;
    Ok(copy.summary)
}

struct Copy<'a> {
    runner: &'a mut dyn CommandRunner,
    progress: &'a mut (dyn FnMut(TransferProgress) + Send),
    cancel: &'a watch::Receiver<bool>,
    summary: TransferSummary,
}

impl Copy<'_> {
    fn check_cancelled(&self) -> Result<()> {
        if *self.cancel.borrow() {
            return Err(anyhow!(TransferFailure::Cancelled));
        }
        Ok(())
    }

    fn report(&mut self, name: &str, done: u64, total: u64) {
        (self.progress)(TransferProgress {
            name: name.to_string(),
            done,
            total,
            files: self.summary.files,
        });
    }

    /// Run a command that takes no input to its end and give back what it
    /// printed.
    async fn run(&mut self, script: &str, arguments: &[&str]) -> Result<Vec<u8>> {
        self.check_cancelled()?;
        let mut running = self.runner.start(command(script, arguments)).await?;
        running.close_input().await?;
        let mut output = Vec::new();
        while let Some(chunk) = running.read().await? {
            output.extend_from_slice(&chunk);
        }
        succeeded(running.finish())?;
        Ok(output)
    }

    async fn make_folder(&mut self, path: &str) -> Result<()> {
        let script = format!(r#"{PATH}mkdir -p "$p""#);
        self.run(&script, &[path]).await.map(|_| ())
    }

    /// Send one file, or a folder with what it holds, into the host's
    /// `folder`. Links are followed.
    async fn send(&mut self, path: &Path, folder: &str) -> Result<()> {
        self.check_cancelled()?;
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .filter(|name| !name.contains(['\n', '\0']))
            .ok_or_else(|| anyhow!(TransferFailure::UnsendableName(path.to_path_buf())))?
            .to_string();
        let target = remote_join(folder, &name);
        let metadata = tokio::fs::metadata(path).await.map_err(local(path))?;
        if metadata.is_dir() {
            let mut children = Vec::new();
            let mut entries = tokio::fs::read_dir(path).await.map_err(local(path))?;
            while let Some(entry) = entries.next_entry().await.map_err(local(path))? {
                children.push(entry.path());
            }
            children.sort();
            self.make_folder(&target).await?;
            for child in children {
                Box::pin(self.send(&child, &target)).await?;
            }
            return Ok(());
        }

        let total = metadata.len();
        let mut file = tokio::fs::File::open(path).await.map_err(local(path))?;
        // Whole or not at all: `cat` cannot tell an upload that was cut
        // short from one that ended, so the size decides.
        let script = format!(
            r#"{PATH}t=$p{PART_SUFFIX}; cat >"$t" && [ "$(wc -c <"$t" | tr -d " ")" = "$2" ] && {{ chmod "$3" "$t" 2>/dev/null; mv -f "$t" "$p"; }} || {{ rm -f "$t"; exit 1; }}"#
        );
        let size = total.to_string();
        let mode = format!("{:o}", mode(&metadata));
        let mut running = self
            .runner
            .start(command(&script, &[&target, &size, &mode]))
            .await?;
        let mut buffer = vec![0; CHUNK];
        let mut done = 0;
        self.report(&name, done, total);
        while done < total {
            self.check_cancelled()?;
            let want = CHUNK.min((total - done) as usize);
            let read = file.read(&mut buffer[..want]).await.map_err(local(path))?;
            if read == 0 {
                break;
            }
            running.write(&buffer[..read]).await?;
            done += read as u64;
            self.report(&name, done, total);
        }
        running.close_input().await?;
        while running.read().await?.is_some() {}
        succeeded(running.finish())?;
        if done < total {
            // The file shrank while it was read; the host has thrown its
            // part away.
            return Err(wrap(
                TransferFailure::Local(path.to_path_buf()),
                std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
            ));
        }
        self.summary.files += 1;
        self.summary.bytes += total;
        Ok(())
    }

    async fn receive(&mut self, source: &str, into: &Path) -> Result<()> {
        // What `source` is, then for a folder its own name, its folders and
        // its files, each list ended by a line of `/`.
        let script = format!(
            r#"{PATH}if [ -d "$p" ]; then echo d; cd "$p" && basename "$(pwd)" && find . -type d && echo / && find . -type f && echo /; elif [ -f "$p" ]; then echo f; else echo n; fi"#
        );
        let listing = self.run(&script, &[source]).await?;
        let listing = String::from_utf8_lossy(&listing);
        let mut lines = listing.lines();
        match lines.next() {
            Some("f") => {
                let name = source.rsplit('/').next().unwrap_or_default();
                let name = safe_name(name)?;
                tokio::fs::create_dir_all(into).await.map_err(local(into))?;
                self.receive_file(source, &into.join(name), name).await
            }
            Some("d") => {
                let unexpected = || anyhow!(TransferFailure::Unexpected);
                let name = lines.next().ok_or_else(unexpected)?;
                // The host's root has no name of its own.
                let root = into.join(safe_name(name).unwrap_or("root"));
                let mut folders = Vec::new();
                let mut files = Vec::new();
                let mut ended = 0;
                for line in lines {
                    if line == "/" {
                        ended += 1;
                        continue;
                    }
                    let relative = line.strip_prefix("./").unwrap_or(line);
                    if relative == "." {
                        continue;
                    }
                    for part in relative.split('/') {
                        safe_name(part)?;
                    }
                    match ended {
                        0 => folders.push(relative.to_string()),
                        1 => files.push(relative.to_string()),
                        _ => return Err(unexpected()),
                    }
                }
                if ended != 2 {
                    return Err(unexpected());
                }
                tokio::fs::create_dir_all(&root)
                    .await
                    .map_err(local(&root))?;
                for folder in folders {
                    let path = root.join(&folder);
                    tokio::fs::create_dir_all(&path)
                        .await
                        .map_err(local(&path))?;
                }
                for file in files {
                    let name = file.rsplit('/').next().unwrap_or_default().to_string();
                    let remote = remote_join(source, &file);
                    self.receive_file(&remote, &root.join(&file), &name).await?;
                }
                Ok(())
            }
            Some("n") => Err(anyhow!(TransferFailure::NotFound(source.to_string()))),
            _ => Err(anyhow!(TransferFailure::Unexpected)),
        }
    }

    /// Take the host's file `source` to `path`, through a file beside it
    /// that takes its place once whole.
    async fn receive_file(&mut self, source: &str, path: &Path, name: &str) -> Result<()> {
        self.check_cancelled()?;
        let part = path.with_file_name(format!(".{name}{PART_SUFFIX}"));
        let result = self.receive_into(source, &part, name).await;
        let result = match result {
            Ok(total) => tokio::fs::rename(&part, path)
                .await
                .map_err(local(path))
                .map(|()| total),
            Err(error) => Err(error),
        };
        match result {
            Ok(total) => {
                self.summary.files += 1;
                self.summary.bytes += total;
                Ok(())
            }
            Err(error) => {
                let _ = tokio::fs::remove_file(&part).await;
                Err(error)
            }
        }
    }

    async fn receive_into(&mut self, source: &str, part: &Path, name: &str) -> Result<u64> {
        // The size on a line of its own, then the bytes.
        let script = format!(r#"{PATH}wc -c <"$p" && cat "$p""#);
        let mut running = self.runner.start(command(&script, &[source])).await?;
        running.close_input().await?;
        let mut file = tokio::fs::File::create(part).await.map_err(local(part))?;
        let mut header = Vec::new();
        let mut total = None;
        let mut done = 0;
        while let Some(chunk) = running.read().await? {
            self.check_cancelled()?;
            let bytes = match total {
                Some(_) => chunk,
                None => {
                    header.extend_from_slice(&chunk);
                    let Some(end) = header.iter().position(|byte| *byte == b'\n') else {
                        if header.len() > 64 {
                            return Err(anyhow!(TransferFailure::Unexpected));
                        }
                        continue;
                    };
                    let size = std::str::from_utf8(&header[..end])
                        .ok()
                        .and_then(|size| size.trim().parse::<u64>().ok())
                        .ok_or_else(|| anyhow!(TransferFailure::Unexpected))?;
                    total = Some(size);
                    self.report(name, 0, size);
                    header.split_off(end + 1)
                }
            };
            file.write_all(&bytes).await.map_err(local(part))?;
            done += bytes.len() as u64;
            self.report(name, done, total.unwrap_or(done));
        }
        succeeded(running.finish())?;
        file.flush().await.map_err(local(part))?;
        match total {
            Some(total) if total == done => Ok(total),
            // It changed size while it was read.
            Some(_) => Err(connection(std::io::Error::from(
                std::io::ErrorKind::UnexpectedEof,
            ))),
            None => Err(anyhow!(TransferFailure::Unexpected)),
        }
    }
}

/// A name of the host's that this end may make in the folder it is in: no
/// path, and not the folder itself or the one above.
fn safe_name(name: &str) -> Result<&str> {
    if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\', '\0']) {
        return Err(anyhow!(TransferFailure::UnsafeName(name.to_string())));
    }
    Ok(name)
}

fn succeeded(finished: Finished) -> Result<()> {
    match finished.code {
        Some(0) => Ok(()),
        Some(_) => Err(anyhow!(TransferFailure::Remote(
            finished.stderr.trim().to_string()
        ))),
        None => Err(connection(std::io::Error::from(
            std::io::ErrorKind::UnexpectedEof,
        ))),
    }
}

/// The permission bits a new file gets on the host.
#[cfg(unix)]
fn mode(metadata: &std::fs::Metadata) -> u32 {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o777
}

#[cfg(not(unix))]
fn mode(_: &std::fs::Metadata) -> u32 {
    0o644
}

/// A command on a channel of a live connection. The channel is one nothing
/// has been asked of yet.
pub struct ChannelCommand {
    channel: Channel<client::Msg>,
    finished: Finished,
    /// Its output has ended.
    ended: bool,
}

impl ChannelCommand {
    pub async fn start(channel: Channel<client::Msg>, command: String) -> Result<Self> {
        channel.exec(true, command).await.map_err(connection)?;
        Ok(Self {
            channel,
            finished: Finished::default(),
            ended: false,
        })
    }
}

impl RunningCommand for ChannelCommand {
    fn write<'a>(&'a mut self, bytes: &'a [u8]) -> BoxFuture<'a, Result<()>> {
        Box::pin(async move { self.channel.data(bytes).await.map_err(connection) })
    }

    fn close_input(&mut self) -> BoxFuture<'_, Result<()>> {
        Box::pin(async move { self.channel.eof().await.map_err(connection) })
    }

    fn read(&mut self) -> BoxFuture<'_, Result<Option<Vec<u8>>>> {
        Box::pin(async move {
            // Until both the end of its output and its exit status have
            // come, in whichever order the server sends them, or the
            // channel closes.
            loop {
                if self.ended && self.finished.code.is_some() {
                    return Ok(None);
                }
                match self.channel.wait().await {
                    Some(ChannelMsg::Data { data }) => return Ok(Some(data.to_vec())),
                    Some(ChannelMsg::ExtendedData { data, ext: 1 }) => {
                        let room = STDERR_LIMIT.saturating_sub(self.finished.stderr.len());
                        let text = String::from_utf8_lossy(&data);
                        self.finished.stderr.extend(text.chars().take(room));
                    }
                    Some(ChannelMsg::ExitStatus { exit_status }) => {
                        self.finished.code = Some(exit_status);
                    }
                    Some(ChannelMsg::Eof) => self.ended = true,
                    Some(ChannelMsg::Failure) => {
                        return Err(anyhow!(TransferFailure::Remote(String::new())));
                    }
                    Some(ChannelMsg::Close) | None => return Ok(None),
                    Some(_) => {}
                }
            }
        })
    }

    fn finish(self: Box<Self>) -> Finished {
        self.finished
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::process::Stdio;
    use tokio::process::{Child, ChildStdin, ChildStdout};

    /// Runs the commands with this machine's `sh`, in `home`, the way a
    /// login shell there would.
    struct LocalShell {
        home: PathBuf,
        commands: Vec<String>,
    }

    struct LocalCommand {
        child: Child,
        stdin: Option<ChildStdin>,
        stdout: ChildStdout,
    }

    impl CommandRunner for LocalShell {
        fn start(&mut self, command: String) -> BoxFuture<'_, Result<Box<dyn RunningCommand>>> {
            Box::pin(async move {
                self.commands.push(command.clone());
                let mut child = tokio::process::Command::new("sh")
                    .args(["-c", &command])
                    .current_dir(&self.home)
                    .env("HOME", &self.home)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .kill_on_drop(true)
                    .spawn()?;
                let stdin = child.stdin.take();
                let stdout = child.stdout.take().expect("piped");
                Ok(Box::new(LocalCommand {
                    child,
                    stdin,
                    stdout,
                }) as Box<dyn RunningCommand>)
            })
        }
    }

    impl RunningCommand for LocalCommand {
        fn write<'a>(&'a mut self, bytes: &'a [u8]) -> BoxFuture<'a, Result<()>> {
            Box::pin(async move {
                let stdin = self.stdin.as_mut().expect("input still open");
                stdin.write_all(bytes).await.map_err(connection)
            })
        }

        fn close_input(&mut self) -> BoxFuture<'_, Result<()>> {
            self.stdin = None;
            Box::pin(async { Ok(()) })
        }

        fn read(&mut self) -> BoxFuture<'_, Result<Option<Vec<u8>>>> {
            Box::pin(async move {
                let mut buffer = vec![0; 8192];
                let read = self.stdout.read(&mut buffer).await?;
                buffer.truncate(read);
                Ok((read > 0).then_some(buffer))
            })
        }

        fn finish(mut self: Box<Self>) -> Finished {
            // The output has ended, so the command has too, or is about to.
            let handle = tokio::runtime::Handle::current();
            tokio::task::block_in_place(|| {
                handle.block_on(async {
                    let mut stderr = String::new();
                    if let Some(mut pipe) = self.child.stderr.take() {
                        let _ = pipe.read_to_string(&mut stderr).await;
                    }
                    let status = self.child.wait().await.ok();
                    Finished {
                        code: status
                            .and_then(|status| status.code())
                            .map(|code| code as u32),
                        stderr,
                    }
                })
            })
        }
    }

    fn shell(home: &Path) -> LocalShell {
        LocalShell {
            home: home.to_path_buf(),
            commands: Vec::new(),
        }
    }

    fn failure(error: &anyhow::Error) -> &TransferFailure {
        crate::find_cause::<TransferFailure>(error).expect("a transfer failure")
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        std::fs::write(root.join("one.txt"), "one").unwrap();
        std::fs::create_dir_all(root.join("folder/inner/hollow")).unwrap();
        std::fs::write(root.join("folder/two words.bin"), vec![7; 200_000]).unwrap();
        std::fs::write(root.join("folder/inner/it's"), "").unwrap();
        dir
    }

    fn assert_same_tree(left: &Path, right: &Path) {
        for name in ["one.txt", "folder/two words.bin", "folder/inner/it's"] {
            assert_eq!(
                std::fs::read(left.join(name)).unwrap(),
                std::fs::read(right.join(name)).unwrap(),
                "{name}"
            );
        }
        assert!(right.join("folder/inner/hollow").is_dir());
    }

    #[test]
    fn a_command_is_one_line_any_login_shell_passes_on() {
        assert_eq!(
            command(r#"echo "$1""#, &["it's", "~/a b"]),
            r#"sh -c 'echo "$1"' sh 'it'\''s' '~/a b'"#
        );
        assert_eq!(remote_join("/srv", "a"), "/srv/a");
        assert_eq!(remote_join("/", "a"), "/a");
        assert_eq!(remote_join("~", "a"), "~/a");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn files_and_folders_go_to_a_real_shell_and_come_back() {
        let local = tree();
        let home = tempfile::tempdir().unwrap();
        let (_stop, cancel) = watch::channel(false);
        let mut runner = shell(home.path());
        let mut seen = Vec::new();

        let sources = [local.path().join("one.txt"), local.path().join("folder")];
        let summary = upload(
            &mut runner,
            &sources,
            "~/up load",
            &mut |progress| seen.push(progress),
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(
            summary,
            TransferSummary {
                files: 3,
                bytes: 200_003
            }
        );
        let uploaded = home.path().join("up load");
        assert_same_tree(local.path(), &uploaded);
        assert!(seen.iter().any(|progress| progress.name == "two words.bin"
            && progress.done > 0
            && progress.done < progress.total));
        // Nothing is left of the files the uploads went through.
        assert!(!uploaded.join(format!("one.txt{PART_SUFFIX}")).exists());
        for line in &runner.commands {
            assert!(!line.contains('\n') && !line.contains('!'), "{line}");
        }

        let back = tempfile::tempdir().unwrap();
        // A file by a path from the login folder, a folder by a full one.
        let summary = download(
            &mut runner,
            "up load/one.txt",
            back.path(),
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();
        assert_eq!(summary, TransferSummary { files: 1, bytes: 3 });
        let folder = uploaded.join("folder");
        download(
            &mut runner,
            folder.to_str().unwrap(),
            back.path(),
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap();
        assert_same_tree(local.path(), back.path());
        assert_eq!(std::fs::read_dir(back.path()).unwrap().count(), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn an_upload_keeps_the_files_permission_bits_and_writes_over() {
        use std::os::unix::fs::PermissionsExt as _;
        let local = tempfile::tempdir().unwrap();
        let script = local.path().join("run.sh");
        std::fs::write(&script, "new").unwrap();
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o750)).unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("run.sh"), "old and longer").unwrap();
        let (_stop, cancel) = watch::channel(false);

        upload(&mut shell(home.path()), &[script], "", &mut |_| {}, &cancel)
            .await
            .unwrap();
        let copy = home.path().join("run.sh");
        assert_eq!(std::fs::read_to_string(&copy).unwrap(), "new");
        assert_eq!(
            std::fs::metadata(&copy).unwrap().permissions().mode() & 0o777,
            0o750
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn what_is_missing_or_refused_is_said() {
        let home = tempfile::tempdir().unwrap();
        let into = tempfile::tempdir().unwrap();
        let (_stop, cancel) = watch::channel(false);
        let mut runner = shell(home.path());

        let error = download(&mut runner, "missing", into.path(), &mut |_| {}, &cancel)
            .await
            .unwrap_err();
        assert_eq!(
            failure(&error),
            &TransferFailure::NotFound("missing".into())
        );

        // A file where the folder should go.
        std::fs::write(home.path().join("taken"), "").unwrap();
        let local = tree();
        let error = upload(
            &mut runner,
            &[local.path().join("one.txt")],
            "taken",
            &mut |_| {},
            &cancel,
        )
        .await
        .unwrap_err();
        assert!(
            matches!(failure(&error), TransferFailure::Remote(message) if !message.is_empty()),
            "{error:?}"
        );
        assert_eq!(std::fs::read_dir(into.path()).unwrap().count(), 0);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_cancelled_upload_leaves_what_was_there() {
        let local = tempfile::tempdir().unwrap();
        let path = local.path().join("big");
        std::fs::write(&path, vec![1; CHUNK * 4]).unwrap();
        let home = tempfile::tempdir().unwrap();
        std::fs::write(home.path().join("big"), "before").unwrap();
        let (stop, cancel) = watch::channel(false);

        let error = upload(
            &mut shell(home.path()),
            &[path],
            "~",
            &mut |progress| {
                if progress.done > 0 {
                    let _ = stop.send(true);
                }
            },
            &cancel,
        )
        .await
        .unwrap_err();
        assert_eq!(failure(&error), &TransferFailure::Cancelled);
        // The command is killed with its runner; what it had written goes
        // unused, and the file is as it was.
        assert_eq!(
            std::fs::read_to_string(home.path().join("big")).unwrap(),
            "before"
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_name_on_the_host_never_leaves_the_folder() {
        for name in ["..", ".", "a/b", "a\\b", ""] {
            assert_eq!(
                failure(&safe_name(name).unwrap_err()),
                &TransferFailure::UnsafeName(name.into())
            );
        }
        assert_eq!(safe_name("a b.txt").unwrap(), "a b.txt");
    }
}
