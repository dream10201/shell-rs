//! 文件传输: a remote terminal's upload and download, over the terminal's
//! own connection. The fake terminal takes the transfers' commands and says
//! each went well; no server and no file picker is touched.

use crate::support::*;
use shellrs::app::{StartDownload, StartUpload};

async fn open(cx: &mut TestAppContext) -> (WindowHandle<Root>, Arc<FakeTerminalFactory>) {
    let factory = Arc::new(FakeTerminalFactory::default());
    let (handle, workspace) =
        open_workspace_with_remote_factory(cx, HostStore::seed(), factory.clone());
    cx.wait_for(handle.into(), Duration::from_secs(2), |_, cx| {
        remote_lifecycle(&workspace, RemoteTerminalId(INITIAL_WEB_TERMINAL), cx).accepts_input()
    })
    .await;
    (handle, factory)
}

#[gpui_kit::test]
async fn a_terminal_offers_upload_and_download_and_an_upload_runs_on_its_connection(
    cx: &mut TestAppContext,
) {
    let (handle, factory) = open(cx).await;
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("app.conf");
    std::fs::write(&file, "listen 80;").unwrap();

    in_frame(cx, handle, |window, cx| {
        window.click(("terminal-tab", INITIAL_WEB_TERMINAL), cx);
    });
    in_frame(cx, handle, |window, cx| {
        assert!(
            window
                .find(("upload-files", INITIAL_WEB_TERMINAL))
                .visible()
        );
        assert!(
            window
                .find(("download-files", INITIAL_WEB_TERMINAL))
                .visible()
        );
        // What the upload dialog starts.
        window.dispatch_action(
            Box::new(StartUpload {
                terminal: RemoteTerminalId(INITIAL_WEB_TERMINAL),
                sources: vec![file.clone()],
                destination: "/etc/app".into(),
            }),
            cx,
        );
    });
    cx.wait_for(handle.into(), Duration::from_secs(5), |window, cx| {
        window.notifications(cx).len() == 1
    })
    .await;
    let commands = factory.transfer_commands();
    // The folder is made, then the file goes to `cat` beside the shell.
    assert_eq!(commands.len(), 2, "{commands:?}");
    assert!(commands[0].contains("mkdir -p") && commands[0].ends_with("'/etc/app'"));
    assert!(commands[1].contains("cat >") && commands[1].contains("'/etc/app/app.conf' '10'"));
    // Nothing was typed into the terminal.
    assert_eq!(factory.written_text(), "");
    in_frame(cx, handle, |window, _| {
        assert!(window.try_find("status-transfers").is_none());
    });
}

#[gpui_kit::test]
async fn a_download_that_finds_nothing_says_so(cx: &mut TestAppContext) {
    let (handle, factory) = open(cx).await;
    let into = tempfile::tempdir().unwrap();
    in_frame(cx, handle, |window, cx| {
        window.dispatch_action(
            Box::new(StartDownload {
                terminal: RemoteTerminalId(INITIAL_WEB_TERMINAL),
                source: "/var/log/syslog".into(),
                into: into.path().to_path_buf(),
            }),
            cx,
        );
    });
    cx.wait_for(handle.into(), Duration::from_secs(5), |window, cx| {
        window.notifications(cx).len() == 1
    })
    .await;
    // The fake prints nothing, which is not what the host's shell answers.
    assert_eq!(factory.transfer_commands().len(), 1);
    assert_eq!(std::fs::read_dir(into.path()).unwrap().count(), 0);
}
