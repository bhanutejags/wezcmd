use std::process::Stdio;

use tokio::process::Command as TokioCommand;
use tokio::time::{Duration, timeout};

use crate::protocol::{Command, Forward, Notify, Open, Vscode};

pub struct ActionConfig;

pub async fn dispatch(command: Command, _config: &ActionConfig) -> Result<(), String> {
    match command {
        Command::Open(Open { url }) => {
            run_argv(
                &["/usr/bin/open", "--", &url],
                Duration::from_secs(10),
                false,
            )
            .await
        }
        Command::Notify(Notify { title, body }) => {
            run_argv(
                &[
                    "/opt/homebrew/bin/terminal-notifier",
                    "-title",
                    &title,
                    "-message",
                    &body,
                ],
                Duration::from_secs(10),
                false,
            )
            .await
        }
        Command::Vscode(Vscode { path, host }) => {
            let target = host;
            let url = if target.is_empty() {
                format!("vscode://file{path}")
            } else {
                format!("vscode://vscode-remote/ssh-remote+{target}{path}")
            };
            run_argv(
                &["/usr/bin/open", "--", &url],
                Duration::from_secs(10),
                false,
            )
            .await
        }
        Command::Forward(Forward { port, host }) => {
            let target = host;
            if target.is_empty() {
                return Err("no forward target host".into());
            }
            if !confirm_forward(port.0, &target).await {
                return Err("denied".into());
            }

            let bind = format!("{}:localhost:{}", port.0, port.0);
            run_argv(
                &[
                    "/usr/bin/ssh",
                    "-f",
                    "-N",
                    "-o",
                    "ExitOnForwardFailure=yes",
                    "-o",
                    "BatchMode=yes",
                    "-L",
                    &bind,
                    &target,
                ],
                Duration::from_secs(10),
                true,
            )
            .await
        }
        Command::ProxyRegister(_)
        | Command::ProxyListen(_)
        | Command::ProxyStop(_)
        | Command::ProxyStream(_) => Err("invalid action".into()),
    }
}

async fn confirm_forward(port: u16, host: &str) -> bool {
    let script = r#"on run argv
  set h to item 1 of argv
  set p to item 2 of argv
  display dialog "A remote session requests forwarding port " & p & " on " & h & " to your Mac. Allow?" buttons {"Deny", "Allow"} default button "Deny" cancel button "Deny" with title "wezcmd"
end run"#;
    run_argv(
        &["/usr/bin/osascript", "-e", script, host, &port.to_string()],
        Duration::from_secs(120),
        false,
    )
    .await
    .is_ok()
}

async fn run_argv(argv: &[&str], wait: Duration, stderr_devnull: bool) -> Result<(), String> {
    let mut cmd = TokioCommand::new(argv[0]);
    cmd.args(&argv[1..])
        .stdout(Stdio::null())
        .stderr(if stderr_devnull {
            Stdio::null()
        } else {
            Stdio::piped()
        })
        .kill_on_drop(true);

    let child = cmd.spawn().map_err(|e| e.to_string())?;
    let output = timeout(wait, child.wait_with_output())
        .await
        .map_err(|_| "timeout".to_string())?
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr)
            .lines()
            .next()
            .unwrap_or("command failed")
            .to_string();
        Err(err)
    }
}
