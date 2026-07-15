use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Result, anyhow};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader, copy_bidirectional};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::{Duration, timeout};

use crate::protocol::{
    Port, ProxyEvent, ProxyListen, ProxyRegister, ProxyStop, ProxyStream, Response,
};

const STREAM_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Default)]
pub struct ProxyState {
    inner: Arc<ProxyInner>,
}

#[derive(Default)]
struct ProxyInner {
    sessions: Mutex<HashMap<String, Session>>,
    listeners: Mutex<HashMap<u16, ListenerEntry>>,
    pending: Mutex<HashMap<PendingKey, oneshot::Sender<UnixStream>>>,
    next_stream: AtomicU64,
}

struct Session {
    token: String,
    events: mpsc::Sender<ProxyEvent>,
}

struct ListenerEntry {
    session: String,
    task: JoinHandle<()>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PendingKey {
    session: String,
    stream: u64,
}

pub async fn register(stream: UnixStream, command: ProxyRegister, state: ProxyState) -> Result<()> {
    let (events, mut receiver) = mpsc::channel::<ProxyEvent>(64);
    {
        let mut sessions = state.inner.sessions.lock().await;
        if sessions.contains_key(&command.session) {
            write_response(stream, Response::error("session exists")).await?;
            return Ok(());
        }
        sessions.insert(
            command.session.clone(),
            Session {
                token: command.token.clone(),
                events,
            },
        );
    }

    let mut stream = stream;
    write_response(&mut stream, Response::ok()).await?;
    eprintln!("[wezcmd] proxy session registered: {}", command.session);

    while let Some(event) = receiver.recv().await {
        let mut payload = serde_json::to_vec(&event)?;
        payload.push(b'\n');
        if stream.write_all(&payload).await.is_err() {
            break;
        }
    }

    cleanup_session(&state, &command.session).await;
    eprintln!("[wezcmd] proxy session stopped: {}", command.session);
    Ok(())
}

pub async fn listen(command: ProxyListen, state: ProxyState) -> Response {
    if let Err(err) = check_session(&state, &command.session, &command.token).await {
        return Response::error(err.to_string());
    }

    let listener = match TcpListener::bind(("127.0.0.1", command.local_port.0)).await {
        Ok(listener) => listener,
        Err(err) => return Response::error(err.to_string()),
    };

    let mut listeners = state.inner.listeners.lock().await;
    if listeners.contains_key(&command.local_port.0) {
        return Response::error("local port already forwarded");
    }

    let state_for_task = state.clone();
    let session = command.session.clone();
    let task = tokio::spawn(async move {
        accept_loop(state_for_task, session, command.remote_port, listener).await;
    });
    listeners.insert(
        command.local_port.0,
        ListenerEntry {
            session: command.session,
            task,
        },
    );
    Response::ok()
}

pub async fn stop(command: ProxyStop, state: ProxyState) -> Response {
    if let Err(err) = check_session(&state, &command.session, &command.token).await {
        return Response::error(err.to_string());
    }
    let mut listeners = state.inner.listeners.lock().await;
    let Some(entry) = listeners.get(&command.local_port.0) else {
        return Response::error("not forwarded");
    };
    if entry.session != command.session {
        return Response::error("owned by another session");
    }
    let entry = listeners
        .remove(&command.local_port.0)
        .expect("entry checked");
    entry.task.abort();
    Response::ok()
}

pub async fn attach_stream(stream: UnixStream, command: ProxyStream, state: ProxyState) {
    if check_session(&state, &command.session, &command.token)
        .await
        .is_err()
    {
        return;
    }
    let key = PendingKey {
        session: command.session,
        stream: command.stream,
    };
    if let Some(sender) = state.inner.pending.lock().await.remove(&key) {
        let _ = sender.send(stream);
    }
}

pub async fn worker(
    socket: &std::path::Path,
    session: String,
    token: String,
    remote_host: String,
) -> Result<()> {
    let mut stream = UnixStream::connect(socket).await?;
    let register = crate::protocol::Command::ProxyRegister(ProxyRegister {
        session: session.clone(),
        token: token.clone(),
    });
    write_command(&mut stream, &register).await?;

    let mut reader = BufReader::new(stream);
    let response = read_response(&mut reader).await?;
    if !response.ok {
        return Err(anyhow!(
            response.err.unwrap_or_else(|| "register failed".into())
        ));
    }

    loop {
        let mut line = Vec::new();
        let n = reader.read_until(b'\n', &mut line).await?;
        if n == 0 {
            break;
        }
        if !line.ends_with(b"\n") {
            break;
        }
        let event: ProxyEvent = serde_json::from_slice(&line[..line.len() - 1])?;
        let socket = socket.to_path_buf();
        let session = session.clone();
        let token = token.clone();
        let remote_host = remote_host.clone();
        tokio::spawn(async move {
            let _ = connect_stream(socket, session, token, remote_host, event).await;
        });
    }
    Ok(())
}

async fn accept_loop(state: ProxyState, session: String, remote_port: Port, listener: TcpListener) {
    loop {
        let Ok((local, _)) = listener.accept().await else {
            break;
        };
        let state_for_task = state.clone();
        let session = session.clone();
        tokio::spawn(async move {
            let _ = bridge_connection(state_for_task, session, remote_port, local).await;
        });
    }
}

async fn bridge_connection(
    state: ProxyState,
    session: String,
    remote_port: Port,
    mut local: TcpStream,
) -> Result<()> {
    let stream = state.inner.next_stream.fetch_add(1, Ordering::Relaxed) + 1;
    let key = PendingKey {
        session: session.clone(),
        stream,
    };
    let (sender, receiver) = oneshot::channel();
    state.inner.pending.lock().await.insert(key.clone(), sender);

    let event_sender = {
        let sessions = state.inner.sessions.lock().await;
        sessions
            .get(&session)
            .map(|session| session.events.clone())
            .ok_or_else(|| anyhow!("session gone"))?
    };
    if event_sender
        .send(ProxyEvent::TcpOpen {
            stream,
            remote_port,
        })
        .await
        .is_err()
    {
        state.inner.pending.lock().await.remove(&key);
        return Err(anyhow!("session gone"));
    }

    let mut unix = timeout(STREAM_CONNECT_TIMEOUT, receiver)
        .await
        .map_err(|_| anyhow!("stream timeout"))?
        .map_err(|_| anyhow!("stream canceled"))?;
    let _ = copy_bidirectional(&mut local, &mut unix).await;
    Ok(())
}

async fn connect_stream(
    socket: std::path::PathBuf,
    session: String,
    token: String,
    remote_host: String,
    event: ProxyEvent,
) -> Result<()> {
    let ProxyEvent::TcpOpen {
        stream,
        remote_port,
    } = event;
    let mut remote = TcpStream::connect((remote_host.as_str(), remote_port.0)).await?;
    let mut unix = UnixStream::connect(socket).await?;
    let command = crate::protocol::Command::ProxyStream(ProxyStream {
        session,
        token,
        stream,
    });
    write_command(&mut unix, &command).await?;
    let _ = copy_bidirectional(&mut unix, &mut remote).await;
    Ok(())
}

async fn check_session(state: &ProxyState, session: &str, token: &str) -> Result<()> {
    let sessions = state.inner.sessions.lock().await;
    let Some(existing) = sessions.get(session) else {
        return Err(anyhow!("unknown session"));
    };
    if existing.token != token {
        return Err(anyhow!("invalid token"));
    }
    Ok(())
}

async fn cleanup_session(state: &ProxyState, session: &str) {
    state.inner.sessions.lock().await.remove(session);

    let mut listeners = state.inner.listeners.lock().await;
    let ports: Vec<u16> = listeners
        .iter()
        .filter_map(|(port, entry)| (entry.session == session).then_some(*port))
        .collect();
    for port in ports {
        if let Some(entry) = listeners.remove(&port) {
            entry.task.abort();
        }
    }

    let mut pending = state.inner.pending.lock().await;
    pending.retain(|key, _| key.session != session);
}

async fn write_response(mut stream: impl AsyncWriteExt + Unpin, response: Response) -> Result<()> {
    let mut body = serde_json::to_vec(&response)?;
    body.push(b'\n');
    stream.write_all(&body).await?;
    Ok(())
}

async fn write_command(stream: &mut UnixStream, command: &crate::protocol::Command) -> Result<()> {
    command.validate()?;
    let mut body = serde_json::to_vec(command)?;
    body.push(b'\n');
    stream.write_all(&body).await?;
    Ok(())
}

async fn read_response(reader: &mut BufReader<UnixStream>) -> Result<Response> {
    let mut line = Vec::new();
    let n = reader.read_until(b'\n', &mut line).await?;
    if n == 0 || !line.ends_with(b"\n") {
        return Err(anyhow!("empty reply"));
    }
    Ok(serde_json::from_slice(&line[..line.len() - 1])?)
}
