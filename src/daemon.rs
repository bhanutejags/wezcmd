use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::Result;
use nix::sys::stat::{Mode, umask};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::signal;
use tokio::time::{Duration, Instant, timeout};

use crate::actions::{ActionConfig, dispatch};
use crate::protocol::{Command, Response};

const MAX_LINE: usize = 8192;

#[derive(Clone)]
pub struct DaemonConfig {
    pub host: String,
    pub socket_path: PathBuf,
    pub idle_timeout: Duration,
    pub confirm_forward: bool,
    pub control_path: String,
}

pub async fn serve(config: DaemonConfig) -> Result<()> {
    prepare_socket(&config.socket_path)?;
    let old_umask = umask(Mode::from_bits_truncate(0o177));
    let listener = UnixListener::bind(&config.socket_path);
    umask(old_umask);
    let listener = listener?;

    let last_active = Arc::new(Mutex::new(Instant::now()));
    eprintln!(
        "[wezcmd] listening on {} (host={})",
        config.socket_path.display(),
        config.host
    );

    let ctrl_c = signal::ctrl_c();
    tokio::pin!(ctrl_c);
    #[cfg(unix)]
    let mut terminate = signal::unix::signal(signal::unix::SignalKind::terminate())?;

    loop {
        tokio::select! {
            accept = listener.accept() => {
                let (stream, _) = accept?;
                let config = config.clone();
                let last_active = Arc::clone(&last_active);
                tokio::spawn(async move {
                    handle_connection(stream, config, last_active).await;
                });
            }
            _ = &mut ctrl_c => break,
            _ = terminate.recv() => break,
            _ = tokio::time::sleep(Duration::from_secs(5)) => {
                if config.idle_timeout.as_secs_f64() > 0.0 {
                    let idle = last_active.lock().expect("last_active poisoned").elapsed();
                    if idle > config.idle_timeout {
                        eprintln!("[wezcmd] idle timeout; shutting down");
                        break;
                    }
                }
            }
        }
    }

    let _ = fs::remove_file(&config.socket_path);
    eprintln!("[wezcmd] stopped");
    Ok(())
}

async fn handle_connection(
    stream: UnixStream,
    config: DaemonConfig,
    last_active: Arc<Mutex<Instant>>,
) {
    let (read, mut write) = stream.into_split();
    let mut reader = BufReader::new(read).take((MAX_LINE + 1) as u64);
    let mut raw = Vec::new();

    let response = match timeout(Duration::from_secs(2), reader.read_until(b'\n', &mut raw)).await {
        Ok(Ok(n)) if n > 0 && raw.ends_with(b"\n") && raw.len() <= MAX_LINE => {
            *last_active.lock().expect("last_active poisoned") = Instant::now();
            match Command::from_json(raw[..raw.len() - 1].as_ref()) {
                Ok(command) => {
                    let action_config = ActionConfig {
                        host: config.host,
                        confirm_forward: config.confirm_forward,
                        control_path: config.control_path,
                    };
                    match dispatch(command, &action_config).await {
                        Ok(()) => Response::ok(),
                        Err(err) => Response::error(err),
                    }
                }
                Err(_) => Response::error("invalid"),
            }
        }
        _ => Response::error("invalid"),
    };

    let body = serde_json::to_vec(&response)
        .unwrap_or_else(|_| b"{\"ok\":false,\"err\":\"error\"}".to_vec());
    let _ = write.write_all(&body).await;
    let _ = write.write_all(b"\n").await;
    let _ = write.shutdown().await;
}

fn prepare_socket(socket_path: &Path) -> Result<()> {
    if let Some(dir) = socket_path.parent() {
        fs::create_dir_all(dir)?;
        fs::set_permissions(dir, fs::Permissions::from_mode(0o700))?;
    }
    match fs::remove_file(socket_path) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
