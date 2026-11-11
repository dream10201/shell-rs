//! Fake terminal transports, local and remote.

use super::*;

type WritesByRun = Arc<Mutex<Vec<(usize, Vec<u8>)>>>;

#[derive(Clone, Copy, Default)]
pub enum FakeBehavior {
    #[default]
    Running,
    ExitFirst,
    FailFirst,
    ReportsOs(HostOs),
    ReportsLatency(Latency),
    /// Prints this after its prompt line, as a program would.
    Prints(&'static str),
    /// A local shell in this folder, as this computer tells.
    InFolder(&'static str),
    /// Runs `rz` when a line of it is typed: says the header `rz` starts
    /// ZMODEM with, and nothing after.
    Rz,
}

/// What lrzsz's `rz` prints first: its greeting, then ZRINIT.
pub const RZ_STARTS: &[u8] = b"rz waiting to receive.**\x18B0100000023be50\r\x8a\x11";

#[derive(Default)]
pub struct FakeTerminalFactory {
    pub starts: AtomicUsize,
    pub behavior: FakeBehavior,
    pub writes: Arc<Mutex<Vec<Vec<u8>>>>,
    pub writes_by_run: WritesByRun,
    pub resizes: Arc<Mutex<Vec<TerminalSize>>>,
    /// What the commands run beside the shell print, one after another; the
    /// last one repeats. With none the requests are dropped, the way a
    /// local PTY drops them.
    pub exec_outputs: Arc<Mutex<std::collections::VecDeque<String>>>,
    /// The commands run beside the shell, in order.
    pub exec_commands: Arc<Mutex<Vec<String>>>,
    /// The commands of file transfers started beside the shell, in order.
    /// Each takes whatever it is given and ends well, printing nothing.
    pub transfer_commands: Arc<Mutex<Vec<String>>>,
    /// How many times the line was broken (发送 Break).
    pub breaks: Arc<AtomicUsize>,
    /// What a file transfer sent ([`TerminalTransportCommand::WriteData`]).
    pub data: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl FakeTerminalFactory {
    /// What file transfers sent, all of it.
    pub fn data(&self) -> Vec<u8> {
        self.data
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .concat()
    }

    pub fn rz() -> Self {
        Self {
            behavior: FakeBehavior::Rz,
            ..Self::default()
        }
    }

    pub fn breaks(&self) -> usize {
        self.breaks.load(Ordering::SeqCst)
    }

    pub fn answering(outputs: &[&str]) -> Self {
        Self {
            exec_outputs: Arc::new(Mutex::new(
                outputs.iter().map(|output| output.to_string()).collect(),
            )),
            ..Self::default()
        }
    }

    pub fn exec_commands(&self) -> Vec<String> {
        self.exec_commands
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn exec_count(&self) -> usize {
        self.exec_commands().len()
    }

    pub fn transfer_commands(&self) -> Vec<String> {
        self.transfer_commands
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }

    pub fn exit_first() -> Self {
        Self {
            behavior: FakeBehavior::ExitFirst,
            ..Self::default()
        }
    }

    pub fn fail_first() -> Self {
        Self {
            behavior: FakeBehavior::FailFirst,
            ..Self::default()
        }
    }

    pub fn reports_os(os: HostOs) -> Self {
        Self {
            behavior: FakeBehavior::ReportsOs(os),
            ..Self::default()
        }
    }

    pub fn printing(text: &'static str) -> Self {
        Self {
            behavior: FakeBehavior::Prints(text),
            ..Self::default()
        }
    }

    pub fn reports_latency(latency: Latency) -> Self {
        Self {
            behavior: FakeBehavior::ReportsLatency(latency),
            ..Self::default()
        }
    }

    pub fn written_text(&self) -> String {
        let bytes: Vec<_> = self
            .writes
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .iter()
            .flatten()
            .copied()
            .collect();
        String::from_utf8(bytes).expect("fake transport receives UTF-8 test input")
    }

    pub fn starts(&self) -> usize {
        self.starts.load(Ordering::SeqCst)
    }
}

impl TerminalTransportFactory for FakeTerminalFactory {
    fn create(&self) -> Box<dyn TerminalTransport> {
        let run = self.starts.fetch_add(1, Ordering::SeqCst) + 1;
        Box::new(FakeTerminalTransport {
            run,
            behavior: self.behavior,
            writes: self.writes.clone(),
            writes_by_run: self.writes_by_run.clone(),
            resizes: self.resizes.clone(),
            exec_outputs: self.exec_outputs.clone(),
            exec_commands: self.exec_commands.clone(),
            transfer_commands: self.transfer_commands.clone(),
            breaks: self.breaks.clone(),
            data: self.data.clone(),
        })
    }
}

pub struct FakeTerminalTransport {
    pub run: usize,
    pub behavior: FakeBehavior,
    pub writes: Arc<Mutex<Vec<Vec<u8>>>>,
    pub writes_by_run: WritesByRun,
    pub resizes: Arc<Mutex<Vec<TerminalSize>>>,
    pub exec_outputs: Arc<Mutex<std::collections::VecDeque<String>>>,
    pub exec_commands: Arc<Mutex<Vec<String>>>,
    pub transfer_commands: Arc<Mutex<Vec<String>>>,
    pub breaks: Arc<AtomicUsize>,
    pub data: Arc<Mutex<Vec<Vec<u8>>>>,
}

impl TerminalTransport for FakeTerminalTransport {
    fn run(
        self: Box<Self>,
        _: TerminalSize,
        commands: mpsc::Receiver<TerminalTransportCommand>,
        events: async_channel::Sender<TerminalTransportEvent>,
    ) -> anyhow::Result<()> {
        if matches!(self.behavior, FakeBehavior::FailFirst) && self.run == 1 {
            anyhow::bail!("测试启动失败");
        }
        events.send_blocking(TerminalTransportEvent::Started)?;
        if let FakeBehavior::ReportsOs(os) = self.behavior {
            events.send_blocking(TerminalTransportEvent::HostOsDetected(os))?;
        }
        if let FakeBehavior::ReportsLatency(latency) = self.behavior {
            events.send_blocking(TerminalTransportEvent::Latency(latency))?;
        }
        if let FakeBehavior::InFolder(folder) = self.behavior {
            events.send_blocking(TerminalTransportEvent::WorkingDirectory(folder.into()))?;
        }
        events.send_blocking(TerminalTransportEvent::Output(
            format!(
                "\x1b]0;测试终端 {}\x07run:{}$ alpha.txt 会议纪要.md\r\n",
                self.run, self.run
            )
            .into_bytes(),
        ))?;
        if let FakeBehavior::Prints(text) = self.behavior {
            events.send_blocking(TerminalTransportEvent::Output(text.as_bytes().to_vec()))?;
        }
        if matches!(self.behavior, FakeBehavior::ExitFirst) && self.run == 1 {
            events.send_blocking(TerminalTransportEvent::Exited {
                code: 9,
                signal: None,
            })?;
            return Ok(());
        }
        // What was typed since the last Enter, for `FakeBehavior::Rz`.
        let mut line = Vec::new();
        while let Ok(command) = commands.recv() {
            match command {
                TerminalTransportCommand::Write(bytes) => {
                    self.writes_by_run
                        .lock()
                        .unwrap()
                        .push((self.run, bytes.clone()));
                    self.writes
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(bytes.clone());
                    let mut answer = None;
                    if matches!(self.behavior, FakeBehavior::Rz) {
                        line.extend_from_slice(&bytes);
                        if line.ends_with(b"\r") {
                            if line == b"rz\r" {
                                answer = Some(RZ_STARTS.to_vec());
                            }
                            line.clear();
                        }
                    }
                    events.send_blocking(TerminalTransportEvent::Output(bytes))?;
                    if let Some(answer) = answer {
                        events.send_blocking(TerminalTransportEvent::Output(answer))?;
                    }
                }
                // A transfer's bytes are not echoed: the other end takes them.
                TerminalTransportCommand::WriteData(bytes) => {
                    self.data
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(bytes);
                }
                TerminalTransportCommand::Resize(size) => {
                    self.resizes
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(size);
                }
                TerminalTransportCommand::Exec(request) => {
                    self.exec_commands
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(request.command.clone());
                    let mut outputs = self
                        .exec_outputs
                        .lock()
                        .unwrap_or_else(|error| error.into_inner());
                    let output = if outputs.len() > 1 {
                        outputs.pop_front()
                    } else {
                        outputs.front().cloned()
                    };
                    if let Some(output) = output {
                        let _ = request.reply.send(Ok(output));
                    }
                }
                TerminalTransportCommand::StartCommand(request) => {
                    self.transfer_commands
                        .lock()
                        .unwrap_or_else(|error| error.into_inner())
                        .push(request.command);
                    let _ = request.reply.send(Ok(Box::new(FakeCommand)));
                }
                TerminalTransportCommand::SendBreak => {
                    self.breaks.fetch_add(1, Ordering::SeqCst);
                }
                TerminalTransportCommand::PromptReply { .. } => {}
                TerminalTransportCommand::Shutdown => break,
            }
        }
        Ok(())
    }
}

/// A file transfer's command that takes its input and ends well.
struct FakeCommand;

impl shellrs::ssh::transfer::RunningCommand for FakeCommand {
    fn write<'a>(&'a mut self, _: &'a [u8]) -> futures::future::BoxFuture<'a, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    }

    fn close_input(&mut self) -> futures::future::BoxFuture<'_, anyhow::Result<()>> {
        Box::pin(async { Ok(()) })
    }

    fn read(&mut self) -> futures::future::BoxFuture<'_, anyhow::Result<Option<Vec<u8>>>> {
        Box::pin(async { Ok(None) })
    }

    fn finish(self: Box<Self>) -> shellrs::ssh::transfer::Finished {
        shellrs::ssh::transfer::Finished {
            code: Some(0),
            stderr: String::new(),
        }
    }
}

/// Local terminals that start, and never print anything.
pub fn fake_local_terminals() -> Arc<FixedLocalTerminalTransportProvider> {
    Arc::new(FixedLocalTerminalTransportProvider::new(Arc::new(
        FakeTerminalFactory::default(),
    )))
}

/// Telnet hosts' terminals that start, and never print anything.
pub fn fake_telnet_terminals() -> Arc<FixedTelnetTerminalTransportProvider> {
    Arc::new(FixedTelnetTerminalTransportProvider::new(Arc::new(
        FakeTerminalFactory::default(),
    )))
}

/// Serial hosts' terminals that start, and never print anything.
pub fn fake_serial_terminals() -> Arc<FixedSerialTerminalTransportProvider> {
    Arc::new(FixedSerialTerminalTransportProvider::new(Arc::new(
        FakeTerminalFactory::default(),
    )))
}

pub fn open_workspace_with_remote_factory(
    cx: &mut TestAppContext,
    store: HostStore,
    factory: Arc<dyn TerminalTransportFactory>,
) -> (WindowHandle<Root>, Entity<Workspace>) {
    open_workspace_with_terminals(cx, store, factory, fake_serial_terminals())
}

/// With `serial` asked for each serial host's terminal, and `factory`
/// giving every SSH host's.
pub fn open_workspace_with_terminals(
    cx: &mut TestAppContext,
    store: HostStore,
    factory: Arc<dyn TerminalTransportFactory>,
    serial: Arc<FixedSerialTerminalTransportProvider>,
) -> (WindowHandle<Root>, Entity<Workspace>) {
    open_workspace_with_protocols(cx, store, factory, fake_telnet_terminals(), serial)
}

/// With `telnet` and `serial` asked for each Telnet and serial host's
/// terminal, and `factory` giving every SSH host's.
pub fn open_workspace_with_protocols(
    cx: &mut TestAppContext,
    store: HostStore,
    factory: Arc<dyn TerminalTransportFactory>,
    telnet: Arc<FixedTelnetTerminalTransportProvider>,
    serial: Arc<FixedSerialTerminalTransportProvider>,
) -> (WindowHandle<Root>, Entity<Workspace>) {
    init_app(cx);
    // Dialogs still, as in every fixture: see `open_workspace_with_tester`.
    cx.update(|cx| cx.set_reduce_motion(true));
    let mut workspace = None;
    let handle = cx.open_window(size(px(1280.), px(800.)), |window, cx| {
        let store = cx.new(|_| store);
        let remote = Arc::new(FixedRemoteTerminalTransportProvider::new(factory.clone()));
        let view = cx.new(|cx| {
            Workspace::new_with_services(
                store,
                cx.new(|_| settings_store()),
                remote,
                telnet.clone(),
                serial.clone(),
                fake_local_terminals(),
                Arc::new(FakeConnectionTester::default()),
                Arc::new(FakeForwardProvider::default()),
                window,
                cx,
            )
        });
        workspace = Some(view.clone());
        Root::new(view, window, cx)
    });
    (handle, workspace.expect("workspace created"))
}

pub fn remote_lifecycle(
    workspace: &Entity<Workspace>,
    id: RemoteTerminalId,
    cx: &App,
) -> TerminalLifecycle {
    workspace
        .read(cx)
        .remote_terminal(id)
        .expect("terminal exists")
        .read(cx)
        .lifecycle(cx)
}

/// Hands every remote terminal the same fake, recording the login each one
/// was started with.
#[derive(Default)]
pub struct RecordingRemoteProvider {
    pub factory: Arc<FakeTerminalFactory>,
    pub logins: Mutex<Vec<HostLogin>>,
}

impl RecordingRemoteProvider {
    pub fn logins(&self) -> Vec<HostLogin> {
        self.logins
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .clone()
    }
}

impl RemoteTerminalTransportProvider for RecordingRemoteProvider {
    fn factory_for(&self, login: &HostLogin) -> SharedTerminalTransportFactory {
        self.logins
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .push(login.clone());
        self.factory.clone()
    }
}
