use std::path::Path;

use anyhow::{Result, bail};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::time::{Duration, timeout};

use crate::protocol::{Command, Response};

pub async fn probe(socket: &Path) -> bool {
    matches!(
        timeout(Duration::from_secs(1), UnixStream::connect(socket)).await,
        Ok(Ok(_))
    )
}

pub async fn send(socket: &Path, command: &Command) -> Result<Response> {
    command.validate()?;
    let mut stream = timeout(Duration::from_secs(2), UnixStream::connect(socket)).await??;
    let mut payload = serde_json::to_vec(command)?;
    payload.push(b'\n');
    timeout(Duration::from_secs(2), stream.write_all(&payload)).await??;

    let mut reply = Vec::new();
    timeout(Duration::from_secs(2), stream.read_to_end(&mut reply)).await??;
    if reply.is_empty() {
        bail!("empty reply");
    }
    let response: Response = serde_json::from_slice(&reply)?;
    Ok(response)
}
