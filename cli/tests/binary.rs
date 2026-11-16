//! End to end with the compiled `weft` binary: two processes pair through a
//! real relay and learn each other's names, then one listens while the
//! other sends and renames itself.

mod common;

use std::path::Path;
use std::process::{Output, Stdio};

use common::{STEP, TEST, start_relay, timed};
use tokio::io::{AsyncBufReadExt, BufReader, Lines};
use tokio::process::{Child, ChildStderr, ChildStdout, Command};

fn weft(state: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_weft"));
    command
        .arg("--state")
        .arg(state)
        .env_remove("WEFT_STATE")
        .env("WEFT_LOG", "info")
        .stdin(Stdio::null())
        .kill_on_drop(true);
    command
}

/// Runs `weft` to completion and asserts it succeeded.
async fn run(state: &Path, args: &[&str]) -> String {
    let output: Output = tokio::time::timeout(STEP, weft(state).args(args).output())
        .await
        .unwrap_or_else(|_| panic!("weft {args:?} timed out"))
        .unwrap();
    assert!(
        output.status.success(),
        "weft {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

/// Runs `weft` to completion and asserts it failed.
async fn fails(state: &Path, args: &[&str]) {
    let output: Output = tokio::time::timeout(STEP, weft(state).args(args).output())
        .await
        .unwrap_or_else(|_| panic!("weft {args:?} timed out"))
        .unwrap();
    assert!(!output.status.success(), "weft {args:?} succeeded");
}

async fn next_line<R: tokio::io::AsyncBufRead + Unpin>(lines: &mut Lines<R>) -> String {
    tokio::time::timeout(STEP, lines.next_line())
        .await
        .expect("no line before the timeout")
        .unwrap()
        .expect("output closed")
}

async fn wait_success(child: &mut Child) {
    let status = tokio::time::timeout(STEP, child.wait())
        .await
        .expect("process did not exit")
        .unwrap();
    assert!(status.success(), "{status}");
}

#[tokio::test(flavor = "multi_thread")]
async fn pair_listen_and_send_with_the_binary() {
    timed(async {
        let url = start_relay().await;
        let temp = tempfile::Builder::new()
            .prefix("weft-e2e-")
            .tempdir()
            .unwrap();
        let (a, b) = (temp.path().join("a"), temp.path().join("b"));
        let a_id = run(&a, &["init", "--relay", &url, "--name", "Laptop"])
            .await
            .trim()
            .to_owned();
        let b_id = run(&b, &["init", "--relay", &url, "--name", "Téléphone"])
            .await
            .trim()
            .to_owned();
        assert_eq!(a_id.len(), 32);
        // Invalid names are refused, and change nothing.
        for name in ["", "line\nbreak", &"x".repeat(65)] {
            fails(&a, &["rename", name]).await;
        }
        fails(&b, &["init", "--relay", &url, "--force", "--name", ""]).await;
        // Without trusted devices, a rename is only saved.
        run(&a, &["rename", "Laptop"]).await;

        // A shows a code; B joins with it.
        let mut pair = weft(&a)
            .arg("pair")
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap();
        let mut pair_out = BufReader::new(pair.stdout.take().unwrap()).lines();
        let code = loop {
            let line = next_line(&mut pair_out).await;
            if line.starts_with("WEFT1:") {
                break line;
            }
        };
        let joined = run(&b, &["join", &code]).await;
        assert_eq!(joined.trim(), &a_id[..8]);
        assert_eq!(next_line(&mut pair_out).await, &b_id[..8]);
        wait_success(&mut pair).await;

        // Each side waited for the other's name before exiting.
        assert_eq!(run(&a, &["devices"]).await, format!("{b_id}  Téléphone\n"));
        assert_eq!(run(&b, &["devices"]).await, format!("{a_id}  Laptop\n"));

        // B listens; A sends.
        let mut listen = weft(&b)
            .arg("listen")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut listen_out: Lines<BufReader<ChildStdout>> =
            BufReader::new(listen.stdout.take().unwrap()).lines();
        let mut listen_err: Lines<BufReader<ChildStderr>> =
            BufReader::new(listen.stderr.take().unwrap()).lines();
        loop {
            let line = next_line(&mut listen_err).await;
            eprintln!("listen: {line}");
            if line.contains("listening for clipboard text") {
                break;
            }
        }
        // A renames itself while B listens.
        run(&a, &["rename", "Laptop 2"]).await;
        loop {
            let line = next_line(&mut listen_err).await;
            eprintln!("listen: {line}");
            if line.contains("device name received") {
                assert!(line.contains("Laptop 2"), "{line}");
                assert!(line.contains(&a_id[..8]), "{line}");
                break;
            }
        }
        assert_eq!(run(&b, &["devices"]).await, format!("{a_id}  Laptop 2\n"));

        // Keep draining stderr so the child never blocks on a full pipe.
        let drain = tokio::spawn(async move {
            while let Ok(Some(line)) = listen_err.next_line().await {
                eprintln!("listen: {line}");
            }
        });

        run(&a, &["send", "hello \"from\" A"]).await;
        let line = next_line(&mut listen_out).await;
        let json: serde_json::Value = serde_json::from_str(&line).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "from": a_id, "text": "hello \"from\" A" })
        );

        let mut send = weft(&a)
            .args(["send", "--stdin"])
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        {
            use tokio::io::AsyncWriteExt;
            let mut stdin = send.stdin.take().unwrap();
            stdin.write_all("two\nlines".as_bytes()).await.unwrap();
        }
        wait_success(&mut send).await;
        let json: serde_json::Value =
            serde_json::from_str(&next_line(&mut listen_out).await).unwrap();
        assert_eq!(json["text"], "two\nlines");

        // Packets (sent here through the library, as A) are printed as one
        // line each; one that is not a JSON object is not printed.
        let mut session = weft_cli::Session::connect(weft_cli::StateDir::new(&a))
            .await
            .unwrap();
        let to = session.state().devices[0].device_id();
        session.send_packet(to, "[1]").await.unwrap();
        session
            .send_packet(to, r#"{"type":"ping","message":"hi"}"#)
            .await
            .unwrap();
        session.settle().await.unwrap();
        session.close().await;
        let json: serde_json::Value =
            serde_json::from_str(&next_line(&mut listen_out).await).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "from": a_id, "packet": { "type": "ping", "message": "hi" } })
        );

        // Ctrl-C stops `listen` cleanly.
        #[cfg(unix)]
        {
            let pid = listen.id().unwrap().to_string();
            let status = Command::new("kill")
                .args(["-INT", &pid])
                .status()
                .await
                .unwrap();
            assert!(status.success());
            wait_success(&mut listen).await;
        }
        #[cfg(not(unix))]
        listen.kill().await.unwrap();
        let _ = tokio::time::timeout(TEST, drain).await;
    })
    .await;
}
